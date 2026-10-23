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

/// The most sealed datagrams a pending handshake holds for its packet session: a client's first
/// frame, past which the session's loss recovery sends again what it dropped.
const EARLY_DATAGRAMS: usize = 8;

/// The most handshakes a server holds pending at once, each a TLS connection for its patience:
/// one past it is refused before it starts, as quinn's `max_incoming` bounds them
/// (11-net-stack/udp.md, A server bounds its pending handshakes).
pub(crate) const MAX_PENDING: usize = 1024;

/// The most stateless resets a server sends in one second, each an HMAC and a send for a datagram
/// any address may make up, as RFC 9000 §10.3 lets an endpoint bound them.
pub(crate) const RESETS_PER_SECOND: u32 = 1024;

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
    /// The pending handshake of each connection ID one holds, which a datagram that comes before
    /// the handshake's end finds.
    pending_ids: BTreeMap<ConnectionId, PendingKey>,
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
    /// The second the stateless resets count in, from its start, and how many went in it.
    resets: (Instant, u32),
    /// The OS's on a socket and a seed's on the emulated network, so a run of a seed repeats;
    /// TLS draws its own from the OS in both.
    random: Random,
}

/// A client's first validated `Initial` of a handshake, of any offset: where it came from, its
/// handshake ID, and the handshake data it carries from its offset.
#[derive(Debug)]
struct FirstPacket {
    from: SocketAddr,
    local: Option<IpAddr>,
    id: [u8; ID_LEN],
    offset: u32,
    data: Bytes,
}

struct Pending {
    id: [u8; ID_LEN],
    from: SocketAddr,
    /// The local address the client's datagrams came to, which the server's answers leave from.
    local: Option<IpAddr>,
    tls: Connection,
    connection_id: ConnectionId,
    initial: CryptoStream,
    handshake: CryptoStream,
    handshake_keys: Option<Keys>,
    one_rtt: Option<Keys>,
    phase: ServerPhase,
    /// The client's sealed datagrams that came before the handshake's end, for its packet session.
    early: Vec<Datagram>,
    started: Instant,
    resend: ResendTimer,
}

/// Where a pending handshake stands: reading the client's hello, which may span `Initial`s, then
/// its flight written, which goes again until the client's end arrives.
#[derive(Debug)]
enum ServerPhase {
    Hello,
    Flight(Vec<Bytes>),
}

/// What an `Initial`'s data did to a pending handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HelloStep {
    /// The hello is not whole yet, or the data held nothing new.
    Waiting,
    /// The hello was whole and the flight went.
    Answered,
    /// TLS refused the hello, or it named no client's parameters: its alert went.
    Failed,
}

impl core::fmt::Debug for Pending {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Pending")
            .field("id", &self.id)
            .field("from", &self.from)
            .field("connection_id", &self.connection_id)
            .field("phase", &self.phase)
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
            pending_ids: BTreeMap::new(),
            timers: BTreeSet::new(),
            holders: BTreeMap::new(),
            counts: BTreeMap::new(),
            accepting: false,
            resets: (Instant::ZERO, 0),
            random,
        }
    }

    /// Whether a stateless reset may go at `now`, within `RESETS_PER_SECOND` of its second, which
    /// it then counts.
    fn take_reset(&mut self, now: Instant) -> bool {
        let (second, sent) = &mut self.resets;
        if now.saturating_duration_since(*second) >= Duration::from_secs(1) {
            *second = now;
            *sent = 0;
        }
        if *sent >= RESETS_PER_SECOND {
            return false;
        }
        *sent += 1;
        true
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
        self.pending_ids.remove(&pending.connection_id);
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
            local,
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
            local,
            tls,
            connection_id,
            initial: CryptoStream::default(),
            handshake: CryptoStream::default(),
            handshake_keys: None,
            one_rtt: None,
            phase: ServerPhase::Hello,
            early: Vec::new(),
            started: now,
            resend: ResendTimer::start(now),
        };
        if pending.take_initial(offset, &data, out) == HelloStep::Failed {
            return;
        }
        self.hold(connection_id, from.ip());
        self.timers
            .insert((pending.deadline(self.patience), (from, id)));
        self.pending_ids.insert(connection_id, (from, id));
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

    fn send(out: &mut Establishing, to: SocketAddr, from_ip: Option<IpAddr>, datagram: Bytes) {
        out.outbound.push(Outbound {
            to,
            from_ip,
            datagram,
        });
    }
}

impl Establish for ServerSide {
    fn receive(&mut self, now: Instant, datagram: Datagram, out: &mut Establishing) -> ReadCost {
        let Datagram {
            from,
            local,
            payload,
        } = datagram;
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
                // A later `Initial` of a pending handshake goes on with its hello, or, once its
                // flight went, asks for it again, as quinn routes a pending connection's later
                // `Initial`s to its `Incoming`.
                if let Some(pending) = self.pending.get_mut(&(from, id)) {
                    match pending.phase {
                        ServerPhase::Hello => {
                            if pending.take_initial(offset, &data, out) == HelloStep::Failed {
                                self.drop_pending((from, id));
                            }
                        }
                        ServerPhase::Flight(_) => pending.resend(out),
                    }
                    return ReadCost::Light;
                }
                if !self.accepting {
                    Self::send(out, from, local, handshake_packet::refuse(id));
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
                    Self::send(out, from, local, handshake_packet::retry(id, &token));
                    return ReadCost::Light;
                }
                if self.pending.len() >= MAX_PENDING || !self.admit(from.ip(), now) {
                    Self::send(out, from, local, handshake_packet::refuse(id));
                    return ReadCost::Light;
                }
                self.start(
                    now,
                    FirstPacket {
                        from,
                        local,
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
                        local: pending.local,
                        protection: Protection::Sealed(Box::new(RustlsSeal::new(keys))),
                        prefix: None,
                        route: Some(pending.connection_id),
                        reset_token: None,
                        binding: Some(binding(&pending.tls)),
                        early: pending.early,
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

    /// Takes a packet session datagram whose connection ID no packet session holds: one of a
    /// pending handshake's ID goes to that handshake, which holds `EARLY_DATAGRAMS` of them for its
    /// packet session and drops the rest, as RFC 9001 §5.7 lets a server keep the 1-RTT packets
    /// that come before its handshake's end; any other gets a stateless reset of that ID's token,
    /// shorter than the datagram, which a datagram of `MIN_RESET_LEN` bytes or less gets none of,
    /// as RFC 9000 §5.2 resets only a packet no connection can claim (11-net-stack/udp.md, A
    /// pending handshake holds its early datagrams).
    fn unknown(&mut self, now: Instant, received: &Datagram, out: &mut Establishing) {
        let Datagram {
            from,
            local,
            payload,
        } = received;
        let datagram: &[u8] = payload;
        let Some(id) = datagram
            .first_chunk()
            .and_then(|bytes| ConnectionId::from_bytes(*bytes))
        else {
            return;
        };
        if let Some(key) = self.pending_ids.get(&id) {
            let pending = self
                .pending
                .get_mut(key)
                .expect("a pending ID's handshake is pending");
            if pending.early.len() < EARLY_DATAGRAMS {
                pending.early.push(received.clone());
            }
            return;
        }
        if datagram.len() <= MIN_RESET_LEN || !self.take_reset(now) {
            return;
        }
        let len = (datagram.len() - 1).min(MAX_RESET_LEN);
        let token = self.reset_key.token(id);
        let mut reset = vec![0; len];
        self.random.fill(&mut reset[..len - token.len()]);
        reset[0] &= 0x7f;
        reset[len - token.len()..].copy_from_slice(&token);
        Self::send(out, *from, *local, Bytes::from(reset));
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

    /// Takes an `Initial`'s `data` from `offset` into the hello, and once TLS read the whole
    /// hello, which names the client's parameters, writes the flight and sends it.
    fn take_initial(&mut self, offset: u32, data: &[u8], out: &mut Establishing) -> HelloStep {
        let mut plain = Vec::new();
        self.initial.push(offset, data, &mut plain);
        if plain.is_empty() {
            return HelloStep::Waiting;
        }
        if self.tls.read_hs(&plain).is_err() {
            self.fail(out);
            return HelloStep::Failed;
        }
        match self.tls.quic_transport_parameters() {
            None => HelloStep::Waiting,
            Some(params) if transport_params::is_client(params) => {
                self.phase = ServerPhase::Flight(self.write_flight());
                self.resend(out);
                HelloStep::Answered
            }
            Some(_) => {
                self.fail(out);
                HelloStep::Failed
            }
        }
    }

    /// What TLS writes after the `ClientHello`: the `ServerHello` in the first epoch, the rest of
    /// the server's flight in the handshake epoch, and the 1-RTT keys.
    fn write_flight(&mut self) -> Vec<Bytes> {
        let mut flight = Vec::new();
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
                        flight.push(handshake_packet::initial(
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
                        flight.push(handshake_packet::handshake(
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
                None => return flight,
            }
        }
    }

    /// Sends its flight again; none while it reads the hello.
    fn resend(&self, out: &mut Establishing) {
        let ServerPhase::Flight(flight) = &self.phase else {
            return;
        };
        for datagram in flight {
            out.outbound.push(Outbound {
                to: self.from,
                from_ip: self.local,
                datagram: datagram.clone(),
            });
        }
    }

    /// Sends the alert TLS raised; the handshake goes with no entity to tell.
    fn fail(&self, out: &mut Establishing) {
        let alert = self.tls.alert().map_or(80, u8::from);
        out.outbound.push(Outbound {
            to: self.from,
            from_ip: self.local,
            datagram: handshake_packet::alert(self.id, alert),
        });
    }
}
