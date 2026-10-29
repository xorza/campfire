use rustls::Side;
use rustls::crypto::ring::cipher_suite::TLS13_CHACHA20_POLY1305_SHA256;
use rustls::quic::{Keys, Version};

use super::*;

const ID: [u8; ID_LEN] = [1, 2, 3, 4, 5, 6, 7, 8];

/// The two ends' keys of a test: TLS's initial keys of a fixed ID, which seal as handshake keys do.
fn keys() -> [Keys; 2] {
    let suite = TLS13_CHACHA20_POLY1305_SHA256.tls13().unwrap();
    let quic = suite.quic.unwrap();
    [Side::Client, Side::Server].map(|side| Keys::initial(Version::V1, suite, quic, &ID, side))
}

/// A client's `Initial` is padded to 1,200 bytes and reads back its token, offset and data; a
/// server's is not padded. Each other type reads back what it wrote.
#[test]
fn each_packet_reads_what_it_wrote() {
    let token = [9; TOKEN_LEN];
    let padded = initial(ID, Some(&token), 5, b"hello", Padding::Padded);
    assert_eq!(padded.len(), PADDED_INITIAL);
    assert_eq!(
        HandshakePacket::read(BytesMut::from(&*padded)),
        Some(HandshakePacket::Initial {
            id: ID,
            token: Some(token),
            offset: 5,
            data: Bytes::from_static(b"hello"),
        })
    );
    let unpadded = initial(ID, None, 0, b"hi", Padding::Unpadded);
    // The header of 10, the token's flag, the frame's 6 and the data's 2.
    assert_eq!(unpadded.len(), 10 + 1 + 6 + 2);
    assert_eq!(
        HandshakePacket::read(BytesMut::from(&*retry(ID, &token))),
        Some(HandshakePacket::Retry { id: ID, token })
    );
    assert_eq!(
        HandshakePacket::read(BytesMut::from(&*refuse(ID))),
        Some(HandshakePacket::Refuse { id: ID })
    );
    assert_eq!(
        HandshakePacket::read(BytesMut::from(&*alert(ID, 49))),
        Some(HandshakePacket::Alert { id: ID, alert: 49 })
    );
}

/// A `Handshake` packet opens with the peer's key to its offset and data, and not when a byte
/// changed or with another key.
#[test]
fn a_handshake_packet_opens_with_the_peers_key_alone() {
    let [client, server] = keys();
    let sealed = handshake(ID, 3, 40, b"finished", client.local.packet.as_ref());
    let read = |bytes: &Bytes| match HandshakePacket::read(BytesMut::from(&**bytes)) {
        Some(HandshakePacket::Handshake {
            id,
            number,
            datagram,
        }) => {
            assert_eq!(id, ID);
            (number, datagram)
        }
        packet => panic!("a Handshake packet reads as one: {packet:?}"),
    };
    let (number, datagram) = read(&sealed);
    assert_eq!(number, 3);
    assert_eq!(
        HandshakePacket::open(number, datagram, server.remote.packet.as_ref()),
        Some(CryptoFrame {
            offset: 40,
            data: Bytes::from_static(b"finished")
        })
    );
    let (number, datagram) = read(&sealed);
    assert_eq!(
        HandshakePacket::open(number, datagram, client.remote.packet.as_ref()),
        None
    );
    let mut changed = BytesMut::from(&*sealed);
    changed[14] ^= 1;
    let (number, datagram) = read(&changed.freeze());
    assert_eq!(
        HandshakePacket::open(number, datagram, server.remote.packet.as_ref()),
        None
    );
}

/// A datagram of another version, a packet session's first byte, a type past the five, or one cut
/// short is no handshake packet.
#[test]
fn a_datagram_that_is_no_handshake_packet_is_refused() {
    let mut other_version = BytesMut::from(&*refuse(ID));
    other_version[1] = VERSION + 1;
    assert_eq!(HandshakePacket::read(other_version), None);
    let mut packet_session = BytesMut::from(&*refuse(ID));
    packet_session[0] = 0x00;
    assert_eq!(HandshakePacket::read(packet_session), None);
    let mut unknown = BytesMut::from(&*refuse(ID));
    unknown[0] = 0x80 | (5 << 4);
    assert_eq!(HandshakePacket::read(unknown), None);
    let cut = BytesMut::from(&retry(ID, &[0; TOKEN_LEN])[..12]);
    assert_eq!(HandshakePacket::read(cut), None);
}
