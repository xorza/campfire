//! A server's side of its handshakes: a Retry for each new address, a token's check, the
//! admission, the server's flight, and a stateless reset for a packet session it no longer holds.

use core::net::{IpAddr, SocketAddr};
use core::time::Duration;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bytes::Bytes;
use campfire_common::Secret;
use campfire_net_core::Instant;
use campfire_net_core::Random;
use campfire_net_packets::{
    ConnectionId, Datagram, Establish, Established, Establishing, MIN_RESET_LEN, Outbound,
    PathCheck, Protection, ReadCost, ServerEstablish,
};
use rustls::quic::{Connection, KeyChange, Keys, ServerConnection, Version};

use crate::address_token::{NEXT_LIFETIME, RETRY_LIFETIME, TokenKey, TokenKind};
use crate::admission::{AddressKey, Admission};
use crate::admission_setup::AdmissionSetup;
use crate::crypto_stream::{CryptoSend, CryptoStream};
use crate::handshake_packet::{
    self, CryptoFrame, HandshakePacket, ID_LEN, PADDED_INITIAL, Padding,
};
use crate::reset_key::ResetKey;
use crate::rustls_seal::RustlsSeal;
use crate::secure_establishment::binding;
use crate::secure_establishment::resend_timer::ResendTimer;
use crate::transport_params::{self, ServerParams, TOKENS};
use crate::used_tokens::UsedTokens;

/// The longest stateless reset a server sends, whatever the packet it answers.
const MAX_RESET_LEN: usize = 64;

/// A pending handshake's client: its address, and the handshake ID it chose.
type PendingKey = (SocketAddr, [u8; ID_LEN]);

/// A server's side of its handshakes.
#[derive(Debug)]
pub(crate) struct ServerSide {
    tls: Arc<rustls::ServerConfig>,
    token_key: TokenKey,
    used: UsedTokens,
    reset_key: ResetKey,
    admission_setup: AdmissionSetup,
    admission: Option<Admission>,
    /// How long a handshake waits for its client before it goes.
    patience: Duration,
    pending: BTreeMap<PendingKey, Pending>,
    /// When each pending handshake's timer is due, the earlier of its resend and its patience's
    /// end, and the handshake: a set, so a handshake that goes takes its timer with it.
    timers: BTreeSet<(Instant, PendingKey)>,
    /// The connection ID of each live packet session and pending handshake, and the address it was
    /// admitted from, which it counts against: a packet session that moved since still counts
    /// there.
    holders: BTreeMap<ConnectionId, IpAddr>,
    /// How many live packet sessions and pending handshakes each address holds, which the admission
    /// limits.
    counts: BTreeMap<AddressKey, u32>,
    accepting: bool,
    /// The OS's on a socket and a seed's on the emulated network, so a run of a seed repeats;
    /// TLS draws its own from the OS in both.
    random: Random,
}

/// A client's validated first packet: where it came from, its handshake ID, and the handshake data
/// it carries from its offset.
#[derive(Debug)]
struct FirstPacket {
    from: SocketAddr,
    id: [u8; ID_LEN],
    offset: u32,
    data: Bytes,
}

struct Pending {
    id: [u8; ID_LEN],
    from: SocketAddr,
    tls: Connection,
    connection_id: ConnectionId,
    initial: CryptoStream,
    handshake: CryptoStream,
    handshake_keys: Option<Keys>,
    one_rtt: Option<Keys>,
    /// The server's flight, which goes again until the client's end arrives.
    flight: Vec<Bytes>,
    started: Instant,
    resend: ResendTimer,
}

impl core::fmt::Debug for Pending {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Pending")
            .field("id", &self.id)
            .field("from", &self.from)
            .field("connection_id", &self.connection_id)
            .finish_non_exhaustive()
    }
}

impl ServerSide {
    pub(crate) fn new(
        tls: Arc<rustls::ServerConfig>,
        reset_key: ResetKey,
        token_secret: &Secret<32>,
        admission: AdmissionSetup,
        patience: Duration,
        random: Random,
    ) -> Self {
        Self {
            tls,
            token_key: TokenKey::new(token_secret.as_bytes()),
            used: UsedTokens::default(),
            reset_key,
            admission_setup: admission,
            admission: None,
            patience,
            pending: BTreeMap::new(),
            timers: BTreeSet::new(),
            holders: BTreeMap::new(),
            counts: BTreeMap::new(),
            accepting: false,
            random,
        }
    }

    /// Whether `ip` may start a handshake at `now`, within its live packet sessions and its bucket.
    fn admit(&mut self, ip: IpAddr, now: Instant) -> bool {
        let counts = &self.counts;
        let admission = self
            .admission
            .get_or_insert_with(|| Admission::new(self.admission_setup, now));
        admission.admit(ip, now, |key| {
            counts
                .get(&key)
                .map_or(0, |&count| usize::try_from(count).unwrap())
        })
    }

    /// Gives `id` to a pending handshake from `ip`, which then counts against its address.
    fn hold(&mut self, id: ConnectionId, ip: IpAddr) {
        self.holders.insert(id, ip);
        if let Some(key) = AddressKey::of(ip) {
            *self.counts.entry(key).or_default() += 1;
        }
    }

    /// Frees `id`, whose packet session or handshake went, and its count.
    fn release(&mut self, id: ConnectionId) {
        let Some(ip) = self.holders.remove(&id) else {
            return;
        };
        if let Some(key) = AddressKey::of(ip)
            && let Some(count) = self.counts.get_mut(&key)
        {
            *count -= 1;
            if *count == 0 {
                self.counts.remove(&key);
            }
        }
    }

    /// Takes the pending handshake of `key` out, with its timer.
    fn take_pending(&mut self, key: PendingKey) -> Option<Pending> {
        let pending = self.pending.remove(&key)?;
        self.timers.remove(&(pending.deadline(self.patience), key));
        Some(pending)
    }

    /// Takes the pending handshake of `key` out, with its timer and its connection ID.
    fn drop_pending(&mut self, key: PendingKey) -> Option<Pending> {
        let pending = self.take_pending(key)?;
        self.release(pending.connection_id);
        Some(pending)
    }

    /// Starts the handshake of a client's validated `first` packet at `now`, within its
    /// address's admission.
    fn start(&mut self, now: Instant, first: FirstPacket, out: &mut Establishing) {
        let FirstPacket {
            from,
            id,
            offset,
            data,
        } = first;
        let connection_id = self.connection_id();
        let random = &mut self.random;
        let token_key = &self.token_key;
        let tokens = core::array::from_fn::<_, TOKENS, _>(|_| {
            let nonce = u64::from_be_bytes(random.array());
            token_key.mint(
                TokenKind::Next,
                from.ip(),
                [0; ID_LEN],
                nonce,
                now + NEXT_LIFETIME,
            )
        });
        let params = ServerParams {
            id: connection_id,
            reset_token: self.reset_key.token(connection_id),
            tokens,
        };
        let Ok(tls) = ServerConnection::new(Arc::clone(&self.tls), Version::V1, params.encode())
        else {
            return;
        };
        let tls = Connection::Server(tls);
        let mut pending = Pending {
            id,
            from,
            tls,
            connection_id,
            initial: CryptoStream::default(),
            handshake: CryptoStream::default(),
            handshake_keys: None,
            one_rtt: None,
            flight: Vec::new(),
            started: now,
            resend: ResendTimer::start(now),
        };
        let mut plain = Vec::new();
        pending.initial.push(offset, &data, &mut plain);
        if pending.tls.read_hs(&plain).is_err()
            || !pending
                .tls
                .quic_transport_parameters()
                .is_some_and(transport_params::is_client)
        {
            pending.fail(out);
            return;
        }
        pending.write_flight();
        pending.resend(out);
        self.hold(connection_id, from.ip());
        self.timers
            .insert((pending.deadline(self.patience), (from, id)));
        self.pending.insert((from, id), pending);
    }

    /// A random connection ID no live packet session or pending handshake holds.
    fn connection_id(&mut self) -> ConnectionId {
        loop {
            let id = ConnectionId::from_random(self.random.array());
            if !self.holders.contains_key(&id) {
                return id;
            }
        }
    }

    fn send(out: &mut Establishing, to: SocketAddr, datagram: Bytes) {
        out.outbound.push(Outbound { to, datagram });
    }
}

impl Establish for ServerSide {
    fn receive(&mut self, now: Instant, datagram: Datagram, out: &mut Establishing) -> ReadCost {
        let Datagram { from, payload } = datagram;
        let len = payload.len();
        let Some(packet) = HandshakePacket::read(payload) else {
            return ReadCost::Light;
        };
        match packet {
            HandshakePacket::Initial {
                id,
                token,
                offset,
                data,
            } => {
                // A client pads each of its `Initial`s, so an answer to one is never larger than
                // it, whatever address it names (RFC 9000 §14.1).
                if len < PADDED_INITIAL {
                    return ReadCost::Light;
                }
                if let Some(pending) = self.pending.get(&(from, id)) {
                    pending.resend(out);
                    return ReadCost::Light;
                }
                if !self.accepting {
                    Self::send(out, from, handshake_packet::refuse(id));
                    return ReadCost::Light;
                }
                let checked =
                    token.and_then(|token| self.token_key.check(&token, from.ip(), id, now));
                let validated = match checked {
                    Some(token) if token.kind == TokenKind::Retry => true,
                    Some(token) => self.used.take(token.nonce, token.expires, now),
                    None => false,
                };
                if !validated {
                    let token = self.token_key.mint(
                        TokenKind::Retry,
                        from.ip(),
                        id,
                        0,
                        now + RETRY_LIFETIME,
                    );
                    Self::send(out, from, handshake_packet::retry(id, &token));
                    return ReadCost::Light;
                }
                if !self.admit(from.ip(), now) {
                    Self::send(out, from, handshake_packet::refuse(id));
                    return ReadCost::Light;
                }
                self.start(
                    now,
                    FirstPacket {
                        from,
                        id,
                        offset,
                        data,
                    },
                    out,
                );
                return ReadCost::Handshake;
            }
            HandshakePacket::Handshake {
                id,
                number,
                datagram,
            } => {
                let key = (from, id);
                let Some(pending) = self.pending.get_mut(&key) else {
                    return ReadCost::Light;
                };
                let Some(keys) = &pending.handshake_keys else {
                    return ReadCost::Light;
                };
                let Some(CryptoFrame { offset, data }) =
                    HandshakePacket::open(number, datagram, keys.remote.packet.as_ref())
                else {
                    return ReadCost::Light;
                };
                let mut plain = Vec::new();
                pending.handshake.push(offset, &data, &mut plain);
                if plain.is_empty() {
                    return ReadCost::Light;
                }
                if pending.tls.read_hs(&plain).is_err() {
                    if let Some(pending) = self.drop_pending(key) {
                        pending.fail(out);
                    }
                    return ReadCost::Light;
                }
                if !pending.tls.is_handshaking()
                    && let Some(keys) = pending.one_rtt.take()
                {
                    // The packet session keeps the connection ID, and its count, from its
                    // handshake.
                    let pending = self.take_pending(key).unwrap();
                    out.established.push(Established {
                        peer: pending.from,
                        protection: Protection::Sealed(Box::new(RustlsSeal::new(keys))),
                        prefix: None,
                        route: Some(pending.connection_id),
                        reset_token: None,
                        binding: Some(binding(&pending.tls)),
                    });
                }
            }
            HandshakePacket::Alert { id, .. } => {
                self.drop_pending((from, id));
            }
            HandshakePacket::Retry { .. } | HandshakePacket::Refuse { .. } => {}
        }
        ReadCost::Light
    }

    /// Sends each pending flight again at its timer, and drops a handshake that waited longer
    /// than its patience.
    fn poll(&mut self, now: Instant, out: &mut Establishing) {
        let patience = self.patience;
        while let Some(&(at, key)) = self.timers.first()
            && at <= now
        {
            let pending = &self.pending[&key];
            if now >= pending.started + patience {
                self.drop_pending(key);
                continue;
            }
            self.timers.pop_first();
            let pending = self.pending.get_mut(&key).unwrap();
            pending.resend.back_off(now);
            pending.resend(out);
            self.timers.insert((pending.deadline(patience), key));
        }
    }

    fn next_deadline(&self) -> Option<Instant> {
        self.timers.first().map(|&(at, _)| at)
    }

    /// A packet session left: its ID and its address's count go.
    fn ended(&mut self, route: Option<ConnectionId>) {
        let route = route.expect("a server's packet session is routed by its connection ID");
        self.release(route);
    }
}

impl ServerEstablish for ServerSide {
    fn set_accepting(&mut self, accepting: bool) {
        self.accepting = accepting;
    }

    /// Answers a packet session datagram whose connection ID no packet session holds with a
    /// stateless reset of that ID's token, shorter than the datagram, which a datagram of
    /// `MIN_RESET_LEN` bytes or less gets none of.
    fn unknown(&mut self, _now: Instant, datagram: &Datagram, out: &mut Establishing) {
        let Datagram { from, payload } = datagram;
        let datagram: &[u8] = payload;
        let Some(id) = datagram
            .first_chunk()
            .and_then(|bytes| ConnectionId::from_bytes(*bytes))
        else {
            return;
        };
        if datagram.len() <= MIN_RESET_LEN {
            return;
        }
        let len = (datagram.len() - 1).min(MAX_RESET_LEN);
        let token = self.reset_key.token(id);
        let mut reset = vec![0; len];
        self.random.fill(&mut reset[..len - token.len()]);
        reset[0] &= 0x7f;
        reset[len - token.len()..].copy_from_slice(&token);
        Self::send(out, *from, Bytes::from(reset));
    }

    /// A server routes its clients' datagrams by the connection ID it gave each, so a client
    /// moves, and its new path is checked.
    fn path_check(&mut self) -> Option<&mut dyn PathCheck> {
        Some(self)
    }
}

impl PathCheck for ServerSide {
    fn challenge(&mut self) -> [u8; 8] {
        self.random.array()
    }
}

impl Pending {
    /// When its timer is due: its resend, or its patience's end if that comes first.
    fn deadline(&self, patience: Duration) -> Instant {
        (self.started + patience).min(self.resend.at())
    }

    /// Takes what TLS writes after the `ClientHello`: the `ServerHello` in the first epoch, the
    /// rest of the server's flight in the handshake epoch, and the 1-RTT keys.
    fn write_flight(&mut self) {
        let mut initial_send = CryptoSend::default();
        let mut handshake_send = CryptoSend::default();
        let mut number = 0_u16;
        loop {
            let mut written = Vec::new();
            let change = self.tls.write_hs(&mut written);
            match &self.handshake_keys {
                None => {
                    for chunk in written.chunks(handshake_packet::INITIAL_DATA) {
                        let offset = initial_send.take(chunk.len());
                        self.flight.push(handshake_packet::initial(
                            self.id,
                            None,
                            offset,
                            chunk,
                            Padding::Unpadded,
                        ));
                    }
                }
                Some(keys) => {
                    for chunk in written.chunks(handshake_packet::HANDSHAKE_DATA) {
                        let offset = handshake_send.take(chunk.len());
                        self.flight.push(handshake_packet::handshake(
                            self.id,
                            number,
                            offset,
                            chunk,
                            keys.local.packet.as_ref(),
                        ));
                        number = number.wrapping_add(1);
                    }
                }
            }
            match change {
                Some(KeyChange::Handshake { keys }) => self.handshake_keys = Some(keys),
                Some(KeyChange::OneRtt { keys, .. }) => self.one_rtt = Some(keys),
                None => return,
            }
        }
    }

    fn resend(&self, out: &mut Establishing) {
        for datagram in &self.flight {
            out.outbound.push(Outbound {
                to: self.from,
                datagram: datagram.clone(),
            });
        }
    }

    /// Sends the alert TLS raised; the handshake goes with no entity to tell.
    fn fail(&self, out: &mut Establishing) {
        let alert = self.tls.alert().map_or(80, u8::from);
        out.outbound.push(Outbound {
            to: self.from,
            datagram: handshake_packet::alert(self.id, alert),
        });
    }
}
