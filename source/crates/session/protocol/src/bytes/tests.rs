use campfire_codec::Binary;

use super::*;

#[test]
fn bytes_encode_as_a_slice_does() {
    // The length 3, then the bytes; empty, the length 0 alone.
    for payload in [&b"abc"[..], b""] {
        let as_bytes = Binary::encode(&Bytes(payload));
        assert_eq!(as_bytes, Binary::encode(payload));
        let read: Bytes<'_> = Binary::decode(&as_bytes).unwrap();
        assert_eq!(read, Bytes(payload));
    }
    assert_eq!(Binary::encode(&Bytes(b"abc")), b"\x03abc");
    // Through a writer, as the log writes it.
    let mut out = Vec::new();
    Binary::encode_into(&Bytes(b"xy"), &mut out);
    assert_eq!(out, b"\x02xy");
}
