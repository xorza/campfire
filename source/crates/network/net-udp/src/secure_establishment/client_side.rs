//! A client's handshake: its `Initial`, a Retry's token, the server's flight, and its own end.

use core::net::SocketAddr;
use core::time::Duration;
use std::sync::Arc;

use bytes::Bytes;
use campfire_net_core::Instant;
use campfire_net_core::Random;
use campfire_net_packets::{
    ChannelBinding, ClientEstablish, ConnectionId, Datagram, Establish, Established, Establishing,
    Outbound, Protection,
};
use campfire_net_payload::UnlinkReason;
use rustls::pki_types::ServerName;
use rustls::quic::{ClientConnection, Connection, KeyChange, Keys, Version};

use crate::address_token::TOKEN_LEN;
use crate::alert;
use crate::crypto_stream::{CryptoSend, CryptoStream};
use crate::handshake_packet::{self, CryptoFrame, HandshakePacket, ID_LEN, Padding};
use crate::rustls_seal::RustlsSeal;
use crate::secure_establishment::binding;
use crate::secure_establishment::resend_timer::ResendTimer;
use crate::transport_params::{self, ServerParams};

/// A client's side of its handshakes, one at a time.
#[derive(Debug)]
pub(crate) struct ClientSide {
    tls: Arc<rustls::ClientConfig>,
    name: ServerName<'static>,
    /// How long a handshake waits for its server before it fails.
    patience: Duration,
    /// The tokens the server's last handshakes gave, for the next connections.
    tokens: Vec<[u8; TOKEN_LEN]>,
    /// The OS's on a socket and a seed's on the emulated network, so a run of a seed repeats;
    /// TLS draws its own from the OS in both.
    random: Random,
    attempt: Option<Attempt>,
    /// The end of the last handshake, which the client sends again while the server sends its
    /// flight again, until it hears the packet session.
    finished: Option<Finished>,
}

struct Attempt {
    id: [u8; ID_LEN],
    server: SocketAddr,
    tls: Connection,
    /// The `ClientHello`, which each `Initial` carries again.
    hello: Vec<u8>,
    token: Option<[u8; TOKEN_LEN]>,
    /// Whether a Retry came, which the client takes once.
    retried: bool,
    /// Whether the server answered with its flight, after which the server sends it again.
    heard: bool,
    initial: CryptoStream,
    handshake: CryptoStream,
    handshake_keys: Option<Keys>,
    handshake_send: CryptoSend,
    sent_number: u16,
    started: Instant,
    resend: ResendTimer,
}

impl core::fmt::Debug for Attempt {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Attempt")
            .field("id", &self.id)
            .field("server", &self.server)
            .field("retried", &self.retried)
            .field("heard", &self.heard)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
struct Finished {
    id: [u8; ID_LEN],
    server: SocketAddr,
    datagrams: Vec<Bytes>,
    until: Instant,
}

impl ClientSide {
    pub(crate) const fn new(
        tls: Arc<rustls::ClientConfig>,
        name: ServerName<'static>,
        patience: Duration,
        random: Random,
    ) -> Self {
        Self {
            tls,
            name,
            patience,
            tokens: Vec::new(),
            random,
            attempt: None,
            finished: None,
        }
    }
}

impl Establish for ClientSide {
    fn receive(&mut self, now: Instant, datagram: Datagram, out: &mut Establishing) {
        let Datagram { from, payload } = datagram;
        let Some(packet) = HandshakePacket::read(payload) else {
            return;
        };
        if let Some(finished) = &self.finished
            && from == finished.server
            && matches!(
                &packet,
                HandshakePacket::Initial { id, .. } | HandshakePacket::Handshake { id, .. }
                    if *id == finished.id
            )
        {
            for datagram in &finished.datagrams {
                out.outbound.push(Outbound {
                    to: finished.server,
                    datagram: datagram.clone(),
                });
            }
            return;
        }
        let Some(attempt) = self.attempt.as_mut() else {
            return;
        };
        if from != attempt.server {
            return;
        }
        let outcome = match packet {
            HandshakePacket::Retry { id, token }
                if id == attempt.id && !attempt.retried && !attempt.heard =>
            {
                attempt.retried = true;
                attempt.token = Some(token);
                attempt.resend.restart(now);
                attempt.send_initial(out);
                Outcome::Waiting
            }
            HandshakePacket::Refuse { id } if id == attempt.id => {
                Outcome::Failed(UnlinkReason::Refused)
            }
            HandshakePacket::Alert { id, alert } if id == attempt.id => {
                Outcome::Failed(alert::unlink_reason(alert))
            }
            HandshakePacket::Initial {
                id, offset, data, ..
            } if id == attempt.id => {
                attempt.heard = true;
                let mut plain = Vec::new();
                attempt.initial.push(offset, &data, &mut plain);
                attempt.read(&plain, out)
            }
            HandshakePacket::Handshake {
                id,
                number,
                datagram,
            } if id == attempt.id => {
                let Some(keys) = &attempt.handshake_keys else {
                    return;
                };
                let Some(CryptoFrame { offset, data }) =
                    HandshakePacket::open(number, datagram, keys.remote.packet.as_ref())
                else {
                    return;
                };
                attempt.heard = true;
                let mut plain = Vec::new();
                attempt.handshake.push(offset, &data, &mut plain);
                attempt.read(&plain, out)
            }
            _ => Outcome::Waiting,
        };
        match outcome {
            Outcome::Waiting => {}
            Outcome::Failed(reason) => {
                self.attempt = None;
                out.failed = Some(reason);
            }
            Outcome::Done(done) => {
                let attempt = self
                    .attempt
                    .take()
                    .expect("a done handshake was an attempt");
                self.tokens = done.params.tokens.to_vec();
                out.established.push(Established {
                    peer: attempt.server,
                    protection: Protection::Sealed(Box::new(RustlsSeal::new(done.keys))),
                    prefix: Some(done.params.id),
                    route: None,
                    reset_token: Some(done.params.reset_token),
                    binding: Some(done.binding),
                });
                self.finished = Some(Finished {
                    id: attempt.id,
                    server: attempt.server,
                    datagrams: done.finished,
                    until: now + self.patience,
                });
            }
        }
    }

    /// Sends the `Initial` again while the server is silent, and fails a handshake that waited
    /// longer than its patience.
    fn poll(&mut self, now: Instant, out: &mut Establishing) {
        if self
            .finished
            .as_ref()
            .is_some_and(|finished| now >= finished.until)
        {
            self.finished = None;
        }
        let Some(attempt) = self.attempt.as_mut() else {
            return;
        };
        if now >= attempt.started + self.patience {
            self.attempt = None;
            out.failed = Some(UnlinkReason::TimedOut);
            return;
        }
        if !attempt.heard && now >= attempt.resend.at() {
            attempt.resend.back_off(now);
            attempt.send_initial(out);
        }
    }

    fn challenge(&mut self) -> [u8; 8] {
        self.random.array()
    }

    fn next_deadline(&self) -> Option<Instant> {
        let attempt = self.attempt.as_ref().map(|attempt| {
            let give_up = attempt.started + self.patience;
            if attempt.heard {
                give_up
            } else {
                give_up.min(attempt.resend.at())
            }
        });
        let finished = self.finished.as_ref().map(|finished| finished.until);
        attempt.into_iter().chain(finished).min()
    }

    fn routes_by_prefix(&self) -> bool {
        false
    }

    fn ended(&mut self, _: Option<ConnectionId>) {}
}

impl ClientEstablish for ClientSide {
    /// Starts a handshake with `server` at `now`: its `Initial`, with a token of the server's
    /// last handshake if one is left.
    fn connect(&mut self, now: Instant, server: SocketAddr, out: &mut Establishing) {
        let tls = ClientConnection::new(
            Arc::clone(&self.tls),
            Version::V1,
            self.name.clone(),
            transport_params::client(),
        );
        let Ok(tls) = tls else {
            out.failed = Some(UnlinkReason::NotOpened);
            return;
        };
        let mut tls = Connection::Client(tls);
        let mut hello = Vec::new();
        let key_change = tls.write_hs(&mut hello);
        debug_assert!(key_change.is_none(), "a ClientHello changes no key");
        let attempt = Attempt {
            id: self.random.array(),
            server,
            tls,
            hello,
            token: self.tokens.pop(),
            retried: false,
            heard: false,
            initial: CryptoStream::default(),
            handshake: CryptoStream::default(),
            handshake_keys: None,
            handshake_send: CryptoSend::default(),
            sent_number: 0,
            started: now,
            resend: ResendTimer::start(now),
        };
        attempt.send_initial(out);
        self.attempt = Some(attempt);
    }
}

/// What a datagram did to a handshake.
enum Outcome {
    Waiting,
    Failed(UnlinkReason),
    Done(Done),
}

/// A handshake that completed: its keys, the server's parameters, its binding, and the client's
/// end, sealed.
struct Done {
    keys: Keys,
    params: ServerParams,
    binding: ChannelBinding,
    finished: Vec<Bytes>,
}

impl Attempt {
    fn send_initial(&self, out: &mut Establishing) {
        out.outbound.push(Outbound {
            to: self.server,
            datagram: handshake_packet::initial(
                self.id,
                self.token.as_ref(),
                0,
                &self.hello,
                Padding::Padded,
            ),
        });
    }

    /// Feeds the server's handshake data to TLS and takes what TLS writes: the handshake keys,
    /// then the client's end and the 1-RTT keys, which complete the handshake.
    fn read(&mut self, plain: &[u8], out: &mut Establishing) -> Outcome {
        if plain.is_empty() {
            return Outcome::Waiting;
        }
        if self.tls.read_hs(plain).is_err() {
            return self.fail(out);
        }
        loop {
            let mut written = Vec::new();
            let change = self.tls.write_hs(&mut written);
            if !written.is_empty() {
                let Some(keys) = &self.handshake_keys else {
                    return self.fail(out);
                };
                let mut datagrams = Vec::new();
                for chunk in written.chunks(handshake_packet::HANDSHAKE_DATA) {
                    let offset = self.handshake_send.take(chunk.len());
                    datagrams.push(handshake_packet::handshake(
                        self.id,
                        self.sent_number,
                        offset,
                        chunk,
                        keys.local.packet.as_ref(),
                    ));
                    self.sent_number = self.sent_number.wrapping_add(1);
                }
                if let Some(KeyChange::OneRtt { keys, .. }) = change {
                    return self.complete(keys, datagrams, out);
                }
                for datagram in datagrams {
                    out.outbound.push(Outbound {
                        to: self.server,
                        datagram,
                    });
                }
            }
            match change {
                Some(KeyChange::Handshake { keys }) => self.handshake_keys = Some(keys),
                Some(KeyChange::OneRtt { keys, .. }) => {
                    return self.complete(keys, Vec::new(), out);
                }
                None => return Outcome::Waiting,
            }
        }
    }

    /// Sends the alert TLS raised, and fails by it.
    fn fail(&self, out: &mut Establishing) -> Outcome {
        let alert = self.tls.alert().map_or(80, u8::from);
        out.outbound.push(Outbound {
            to: self.server,
            datagram: handshake_packet::alert(self.id, alert),
        });
        Outcome::Failed(alert::unlink_reason(alert))
    }

    fn complete(&self, keys: Keys, finished: Vec<Bytes>, out: &mut Establishing) -> Outcome {
        let params = self
            .tls
            .quic_transport_parameters()
            .and_then(ServerParams::decode);
        let (Some(params), false) = (params, self.tls.is_handshaking()) else {
            return self.fail(out);
        };
        for datagram in &finished {
            out.outbound.push(Outbound {
                to: self.server,
                datagram: datagram.clone(),
            });
        }
        Outcome::Done(Done {
            keys,
            params,
            binding: binding(&self.tls),
            finished,
        })
    }
}
