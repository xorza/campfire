use bytes::{Bytes, BytesMut};
use campfire_net_core::Instant;
use campfire_net_core::Random;
use campfire_net_packets::{
    ClientEstablish, ConnectionId, Datagram, Establish, Established, Establishing, Outbound,
    Protection, ServerEstablish,
};
use campfire_net_payload::UnlinkReason;
use core::net::SocketAddr;
use core::num::NonZeroU32;
use core::time::Duration;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{self, ring};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use std::sync::Arc;
use zeroize::Zeroizing;

use crate::admission_setup::AdmissionSetup;
use crate::certificate_chain::CertificateChain;
use crate::certificate_error::CertificateError;
use crate::handshake_packet::HandshakePacket;
use crate::secure_establishment::client_side::ClientSide;
use crate::secure_establishment::server_side::ServerSide;
use crate::server_identity::ServerIdentity;
use crate::server_secret::ServerSecret;
use crate::server_trust::ServerTrust;

const ALPN: &[u8] = b"campfire-test";

/// How long a test's handshakes wait for their peer.
const PATIENCE: Duration = Duration::from_secs(5);

const SELF_SIGNED: &[u8] = include_bytes!("../fixtures/self_signed.der");
const SELF_SIGNED_KEY: &[u8] = include_bytes!("../fixtures/self_signed_key.der");

/// RFC 8410 §10.3's Ed25519 private key, which signs for no P-256 certificate.
const ED25519_KEY: [u8; 48] = [
    0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
    0xd4, 0xee, 0x72, 0xdb, 0xf9, 0x13, 0x58, 0x4a, 0xd5, 0xb6, 0xd8, 0xf1, 0xf7, 0x69, 0xf8, 0xad,
    0x3a, 0xfe, 0x7c, 0x28, 0xcb, 0xf1, 0xd4, 0xfb, 0xe0, 0x97, 0xa8, 0x8f, 0x44, 0x75, 0x58, 0x42,
];

fn address(last: u8, port: u16) -> SocketAddr {
    SocketAddr::from(([10, 0, 0, last], port))
}

fn identity() -> ServerIdentity {
    ServerIdentity::raw_key(ServerSecret::new(Zeroizing::new([7; 32])))
}

fn pinned() -> ServerTrust {
    ServerTrust::pinned(identity().server_key().unwrap())
}

fn roomy() -> AdmissionSetup {
    AdmissionSetup::new(NonZeroU32::new(8).unwrap(), Duration::from_secs(1))
}

fn at(millis: u64) -> Instant {
    Instant::from_elapsed(Duration::from_millis(millis))
}

/// What a handshake packet is, by its first byte; a packet session's datagram has no type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Initial,
    Retry,
    Handshake,
    Refuse,
    Alert,
    PacketSession,
}

fn kind(datagram: &[u8]) -> Kind {
    match datagram[0] {
        byte if byte & 0x80 == 0 => Kind::PacketSession,
        byte => [
            Kind::Initial,
            Kind::Retry,
            Kind::Handshake,
            Kind::Refuse,
            Kind::Alert,
        ][usize::from((byte >> 4) & 0x07)],
    }
}

/// A client's establishment and a server's, whose datagrams cross in memory, each logged by its
/// kind, and each dropped while its direction's drop count lasts.
#[derive(Debug)]
struct Exchange {
    client: ClientSide,
    server: ServerSide,
    client_address: SocketAddr,
    server_address: SocketAddr,
    client_out: Establishing,
    server_out: Establishing,
    sent: [Vec<Kind>; 2],
    /// Each datagram of each direction, client's first.
    datagrams: [Vec<Bytes>; 2],
    /// The datagrams to drop of each direction, client's first, by their index in it.
    drop: [Vec<usize>; 2],
}

impl Exchange {
    fn new(trust: &ServerTrust, identity: &ServerIdentity, admission: AdmissionSetup) -> Self {
        Self::with_client(
            ClientSide::new(
                trust.tls_config(ALPN),
                trust.name(),
                PATIENCE,
                Random::seeded(1),
            ),
            identity,
            admission,
        )
    }

    fn with_client(
        client: ClientSide,
        identity: &ServerIdentity,
        admission: AdmissionSetup,
    ) -> Self {
        let mut server = ServerSide::new(
            identity.tls_config(ALPN),
            identity.reset_key(),
            &[3; 32],
            admission,
            PATIENCE,
            Random::seeded(2),
        );
        server.set_accepting(true);
        Self {
            client,
            server,
            client_address: address(2, 5000),
            server_address: address(1, 443),
            client_out: Establishing::default(),
            server_out: Establishing::default(),
            sent: [Vec::new(), Vec::new()],
            datagrams: [Vec::new(), Vec::new()],
            drop: [Vec::new(), Vec::new()],
        }
    }

    fn connect(&mut self, now: Instant) {
        self.client
            .connect(now, self.server_address, &mut self.client_out);
    }

    /// Delivers each datagram both sides send at `now`, until neither sends: what each
    /// established.
    fn run(&mut self, now: Instant) -> [Vec<Established>; 2] {
        let mut established = [Vec::new(), Vec::new()];
        loop {
            let from_client = core::mem::take(&mut self.client_out.outbound);
            let from_server = core::mem::take(&mut self.server_out.outbound);
            established[0].append(&mut self.client_out.established);
            established[1].append(&mut self.server_out.established);
            if from_client.is_empty() && from_server.is_empty() {
                return established;
            }
            for Outbound { to, datagram } in from_client {
                assert_eq!(to, self.server_address);
                if self.log(0, &datagram) {
                    self.server.receive(
                        now,
                        Datagram {
                            from: self.client_address,
                            payload: BytesMut::from(&*datagram),
                        },
                        &mut self.server_out,
                    );
                }
            }
            for Outbound { to, datagram } in from_server {
                assert_eq!(to, self.client_address);
                if self.log(1, &datagram) {
                    self.client.receive(
                        now,
                        Datagram {
                            from: self.server_address,
                            payload: BytesMut::from(&*datagram),
                        },
                        &mut self.client_out,
                    );
                }
            }
        }
    }

    /// Logs `datagram` of direction `side`: whether it goes through.
    fn log(&mut self, side: usize, datagram: &Bytes) -> bool {
        let index = self.sent[side].len();
        self.sent[side].push(kind(datagram));
        self.datagrams[side].push(datagram.clone());
        !self.drop[side].contains(&index)
    }

    fn poll(&mut self, now: Instant) {
        self.client.poll(now, &mut self.client_out);
        self.server.poll(now, &mut self.server_out);
    }
}

/// A client's first handshake takes a Retry, then its `Initial` again with the Retry's token; the
/// server's flight is its `ServerHello` and one sealed packet; the client's end is one more. Both
/// ends then hold a sealed packet session: the client starts its datagrams with the server's ID and
/// knows the reset token of it, the server routes by that ID, and both read one binding.
#[test]
fn a_first_handshake_takes_a_retry_then_seals_a_packet_session_both_ends_bind() {
    let mut exchange = Exchange::new(&pinned(), &identity(), roomy());
    exchange.connect(at(0));
    let [client, server] = exchange.run(at(0));
    assert_eq!(
        exchange.sent,
        [
            vec![Kind::Initial, Kind::Initial, Kind::Handshake],
            vec![Kind::Retry, Kind::Initial, Kind::Handshake],
        ]
    );
    let [client] = &*client else {
        panic!("the client holds one packet session: {client:?}");
    };
    let [server] = &*server else {
        panic!("the server holds one packet session: {server:?}");
    };
    let id = server.route.unwrap();
    assert_eq!(client.prefix, Some(id));
    assert_eq!(client.reset_token, Some(identity().reset_key().token(id)));
    assert_eq!(client.peer, exchange.server_address);
    assert_eq!(server.peer, exchange.client_address);
    assert!(matches!(client.protection, Protection::Sealed(_)));
    assert!(client.binding.is_some());
    assert_eq!(client.binding, server.binding);
    assert_eq!(exchange.client.next_deadline(), Some(at(0) + PATIENCE));
    assert_eq!(exchange.server.next_deadline(), None);
}

/// The next handshake carries a token of the last one, which skips the Retry. The same token in
/// an `Initial` of another handshake is spent, and takes a Retry.
#[test]
fn a_token_of_the_last_handshake_skips_the_retry_once() {
    let mut exchange = Exchange::new(&pinned(), &identity(), roomy());
    exchange.connect(at(0));
    exchange.run(at(0));
    exchange.sent = [Vec::new(), Vec::new()];
    exchange.connect(at(10));
    let initial = exchange.client_out.outbound[0].datagram.clone();
    let [client, server] = exchange.run(at(10));
    assert_eq!([client.len(), server.len()], [1, 1]);
    assert_eq!(exchange.sent[1], [Kind::Initial, Kind::Handshake]);

    let mut replayed = BytesMut::from(&*initial);
    replayed[2] ^= 1;
    exchange.server.receive(
        at(20),
        Datagram {
            from: exchange.client_address,
            payload: replayed,
        },
        &mut exchange.server_out,
    );
    let kinds: Vec<_> = exchange
        .server_out
        .outbound
        .iter()
        .map(|outbound| kind(&outbound.datagram))
        .collect();
    assert_eq!(kinds, [Kind::Retry]);
}

/// A client that pins another key refuses the server with `access_denied`: it fails as a wrong
/// server key, its alert drops the server's handshake, and neither holds a packet session.
#[test]
fn a_wrong_pin_refuses_the_server_by_access_denied() {
    let other = ServerIdentity::raw_key(ServerSecret::new(Zeroizing::new([8; 32])));
    let mut exchange = Exchange::new(
        &ServerTrust::pinned(other.server_key().unwrap()),
        &identity(),
        roomy(),
    );
    exchange.connect(at(0));
    let [client, server] = exchange.run(at(0));
    assert!(client.is_empty() && server.is_empty());
    assert_eq!(exchange.sent[0].last(), Some(&Kind::Alert));
    assert_eq!(
        exchange.client_out.failed,
        Some(UnlinkReason::WrongServerKey)
    );
    assert_eq!(exchange.server.next_deadline(), None);
}

/// A certificate no root signed is refused as a wrong server key by a WebPKI client, which sends
/// `unknown_ca`. A chain needs a certificate and its own key, and names why it refuses another.
#[test]
fn a_certificate_no_root_signed_is_refused_as_a_wrong_server_key() {
    let chain =
        |certificates: Vec<Vec<u8>>, key: &[u8]| CertificateChain::new(certificates, key.to_vec());
    assert!(matches!(
        chain(Vec::new(), SELF_SIGNED_KEY),
        Err(CertificateError::Empty)
    ));
    assert!(matches!(
        chain(vec![SELF_SIGNED.to_vec()], &[1, 2, 3]),
        Err(CertificateError::KeyFormat)
    ));
    assert!(matches!(
        chain(vec![SELF_SIGNED.to_vec()], &ED25519_KEY),
        Err(CertificateError::Key(_))
    ));
    let identity = ServerIdentity::certificate(
        ServerSecret::new(Zeroizing::new([7; 32])),
        chain(vec![SELF_SIGNED.to_vec()], SELF_SIGNED_KEY).unwrap(),
    );
    assert_eq!(identity.server_key(), None);
    let trust = ServerTrust::web_pki("localhost").unwrap();
    assert_eq!(trust.pinned_key(), None);
    let mut exchange = Exchange::new(&trust, &identity, roomy());
    exchange.connect(at(0));
    let [client, _] = exchange.run(at(0));
    assert!(client.is_empty());
    assert_eq!(
        exchange.client_out.failed,
        Some(UnlinkReason::WrongServerKey)
    );
    assert_eq!(
        ServerTrust::web_pki("not a name!").unwrap_err().name,
        "not a name!"
    );
}

/// A server that does not accept refuses each handshake, before its Retry.
#[test]
fn a_server_that_does_not_accept_refuses() {
    let mut exchange = Exchange::new(&pinned(), &identity(), roomy());
    exchange.server.set_accepting(false);
    exchange.connect(at(0));
    exchange.run(at(0));
    assert_eq!(exchange.sent[1], [Kind::Refuse]);
    assert_eq!(exchange.client_out.failed, Some(UnlinkReason::Refused));
}

/// An address that holds as many live packet sessions as its admission lets is refused another,
/// after its Retry and before its handshake costs a signature; once the packet session left, it
/// takes one again. The bucket refills in 1 ms, so only the live packet session refuses.
#[test]
fn an_address_past_its_admission_is_refused() {
    let one = AdmissionSetup::new(NonZeroU32::MIN, Duration::from_millis(1));
    let mut exchange = Exchange::new(&pinned(), &identity(), one);
    exchange.connect(at(0));
    let [_, server] = exchange.run(at(0));
    assert_eq!(server.len(), 1);
    // A second client from the same IP, its own port.
    let mut second = Exchange::with_client(
        ClientSide::new(
            pinned().tls_config(ALPN),
            pinned().name(),
            PATIENCE,
            Random::seeded(5),
        ),
        &identity(),
        one,
    );
    core::mem::swap(&mut second.server, &mut exchange.server);
    second.client_address = address(2, 5001);
    second.connect(at(10));
    second.run(at(10));
    assert_eq!(second.sent[1], [Kind::Retry, Kind::Refuse]);
    assert_eq!(second.client_out.failed, Some(UnlinkReason::Refused));
    // The first packet session left: the address takes one again.
    second.server.ended(server[0].route);
    second.client_out.failed = None;
    second.sent = [Vec::new(), Vec::new()];
    second.connect(at(20));
    let [client, _] = second.run(at(20));
    assert_eq!(client.len(), 1);
}

/// A datagram whose connection ID no packet session holds gets a stateless reset of that ID's
/// token, a byte shorter and a packet session's first bit; one of 21 bytes or less gets none.
#[test]
fn an_unknown_connection_id_gets_a_reset_of_its_token() {
    let mut exchange = Exchange::new(&pinned(), &identity(), roomy());
    let id = ConnectionId::from_random([0, 0, 0, 0, 0, 0, 0, 7]);
    let mut unknown = id.to_bytes().to_vec();
    unknown.resize(40, 0x55);
    exchange.server.unknown(
        at(0),
        &Datagram {
            from: exchange.client_address,
            payload: BytesMut::from(&*unknown),
        },
        &mut exchange.server_out,
    );
    let [reset] = &*exchange.server_out.outbound else {
        panic!("one reset answers the datagram");
    };
    assert_eq!(reset.datagram.len(), 39);
    assert_eq!(reset.datagram[0] & 0x80, 0);
    assert_eq!(reset.datagram[23..], identity().reset_key().token(id));
    exchange.server_out.outbound.clear();
    exchange.server.unknown(
        at(0),
        &Datagram {
            from: exchange.client_address,
            payload: BytesMut::from(&unknown[..21]),
        },
        &mut exchange.server_out,
    );
    assert!(exchange.server_out.outbound.is_empty());
}

/// A lost `Initial` goes again at the handshake's timer, 999 ms, and a lost end of the client's
/// goes again when the server sends its flight again at its own; the packet session then opens.
#[test]
fn lost_handshake_packets_go_again_at_their_timers() {
    let mut exchange = Exchange::new(&pinned(), &identity(), roomy());
    exchange.drop = [vec![0, 3], Vec::new()];
    exchange.connect(at(0));
    exchange.run(at(0));
    assert_eq!(exchange.sent[0], [Kind::Initial]);
    exchange.poll(at(998));
    assert!(exchange.client_out.outbound.is_empty());
    exchange.poll(at(999));
    let [client, server] = exchange.run(at(999));
    // The client's Initial again, its Initial with the Retry's token, and its end, dropped.
    assert_eq!(
        exchange.sent[0],
        [Kind::Initial, Kind::Initial, Kind::Initial, Kind::Handshake]
    );
    assert_eq!([client.len(), server.len()], [1, 0]);
    exchange.poll(at(999 + 999));
    let [_, server] = exchange.run(at(999 + 999));
    assert_eq!(server.len(), 1);
}

/// A client of a server whose identity is a certificate links when its verifier takes that
/// certificate.
#[test]
fn a_certificate_chain_links_a_client_whose_verifier_takes_it() {
    let identity = ServerIdentity::certificate(
        ServerSecret::new(Zeroizing::new([7; 32])),
        CertificateChain::new(vec![SELF_SIGNED.to_vec()], SELF_SIGNED_KEY.to_vec()).unwrap(),
    );
    let tls = ServerTrust::tls_config_with(Arc::new(AnyCertificate), ALPN);
    let client = ClientSide::new(
        tls,
        ServerTrust::web_pki("localhost").unwrap().name(),
        PATIENCE,
        Random::seeded(1),
    );
    let mut exchange = Exchange::with_client(client, &identity, roomy());
    exchange.connect(at(0));
    let [client, server] = exchange.run(at(0));
    assert_eq!([client.len(), server.len()], [1, 1]);
}

/// A verifier that takes the fixture's certificate whatever its name and date, and checks the
/// handshake's signatures with `ring`.
#[derive(Debug)]
struct AnyCertificate;

impl ServerCertVerifier for AnyCertificate {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        assert_eq!(end_entity.as_ref(), SELF_SIGNED);
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &CertificateDer<'_>,
        _: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        unreachable!("TLS 1.3 alone")
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// A client's first `Initial`, as it is and cut to 1,199 bytes: the server answers the padded one
/// with a Retry, and the short one with nothing, as an answer to it could be larger than it.
#[test]
fn an_initial_under_1200_bytes_gets_no_answer() {
    let mut exchange = Exchange::new(&pinned(), &identity(), roomy());
    exchange.connect(at(0));
    let initial = exchange.client_out.outbound[0].datagram.clone();
    assert_eq!(initial.len(), 1_200);
    let answers = |exchange: &mut Exchange, datagram: &[u8]| {
        exchange.server.receive(
            at(0),
            Datagram {
                from: exchange.client_address,
                payload: BytesMut::from(datagram),
            },
            &mut exchange.server_out,
        );
        core::mem::take(&mut exchange.server_out.outbound)
            .into_iter()
            .map(|outbound| kind(&outbound.datagram))
            .collect::<Vec<_>>()
    };
    assert_eq!(answers(&mut exchange, &initial[..1_199]), []);
    assert_eq!(answers(&mut exchange, &initial), [Kind::Retry]);
}

/// A seeded mutation test of the handshake: 1,024 mutations of each datagram of a handshake
/// reach a server or a client, before the handshake and after it, and none panics. Each comes
/// from an address of its own, which no token names, so the server answers it with no more bytes
/// than it received.
#[test]
fn a_mutated_handshake_datagram_is_refused_or_read() {
    use campfire_net_bytes::internals::Mutations;

    let mut done = Exchange::new(&pinned(), &identity(), roomy());
    done.connect(at(0));
    done.run(at(0));
    let [from_client, from_server] = done.datagrams.clone();
    let mut fresh = Exchange::new(&pinned(), &identity(), roomy());
    fresh.connect(at(0));

    let mut mutations = Mutations::new(0x00a1_1ce5);
    let mut mutated = Vec::new();
    let mut port = 6_000;
    for exchange in [&mut fresh, &mut done] {
        for original in &from_client {
            for _ in 0..1_024 {
                mutations.mutate(original, &mut mutated);
                let _ = HandshakePacket::read(BytesMut::from(&*mutated));
                port += 1;
                exchange.server.receive(
                    at(1),
                    Datagram {
                        from: address(3, port),
                        payload: BytesMut::from(&*mutated),
                    },
                    &mut exchange.server_out,
                );
                let answered: usize = core::mem::take(&mut exchange.server_out.outbound)
                    .iter()
                    .map(|outbound| outbound.datagram.len())
                    .sum();
                assert!(
                    answered <= mutated.len(),
                    "{answered} bytes answered {} from an unchecked address",
                    mutated.len()
                );
            }
        }
        for original in &from_server {
            for _ in 0..1_024 {
                mutations.mutate(original, &mut mutated);
                let _ = HandshakePacket::read(BytesMut::from(&*mutated));
                exchange.client.receive(
                    at(1),
                    Datagram {
                        from: exchange.server_address,
                        payload: BytesMut::from(&*mutated),
                    },
                    &mut exchange.client_out,
                );
                exchange.client_out.outbound.clear();
                exchange.client_out.established.clear();
            }
        }
    }
}

/// The packet session's ChaCha20-Poly1305 seal of a handshake opens what it sealed, and no mutation
/// that changed a byte of the header it authenticates, of the payload or of the tag.
#[test]
fn the_seal_refuses_every_changed_datagram() {
    use campfire_net_bytes::internals::Mutations;
    use campfire_net_payload::PacketNumber;

    let mut exchange = Exchange::new(&pinned(), &identity(), roomy());
    exchange.connect(at(0));
    let [mut client, mut server] = exchange.run(at(0));
    let (Protection::Sealed(sealer), Protection::Sealed(opener)) =
        (client.remove(0).protection, server.remove(0).protection)
    else {
        panic!("both ends hold a sealed packet session");
    };
    let header = [0x08, 0x00, 0x05];
    let number = PacketNumber(5);
    let mut sealed = b"a payload".to_vec();
    let tag = sealer.seal(number, &header, &mut sealed);
    sealed.extend_from_slice(&tag);
    let mut original = header.to_vec();
    original.extend_from_slice(&sealed);
    let open = |datagram: &[u8]| {
        let (header, sealed) = datagram.split_at(header.len().min(datagram.len()));
        opener.open(number, header, &mut sealed.to_vec())
    };
    assert_eq!(open(&original), Ok(9));

    let mut mutations = Mutations::new(0x5ea1);
    let mut mutated = Vec::new();
    for _ in 0..4_096 {
        mutations.mutate(&original, &mut mutated);
        if mutated != original {
            assert!(open(&mutated).is_err(), "{mutated:02x?} opened");
        }
    }
}
