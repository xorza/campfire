use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A payload's bytes, which serialize as bytes in one write, where a `&[u8]` serializes as a
/// sequence, a write for each `u8`. Postcard encodes both alike, a varint length and then the
/// bytes, so the wire does not change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Bytes<'a>(pub(crate) &'a [u8]);

impl Serialize for Bytes<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(self.0)
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for Bytes<'a> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Bytes<'a>, D::Error> {
        <&'a [u8]>::deserialize(deserializer).map(Bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_encode_as_a_slice_does() {
        // The length 3, then the bytes; empty, the length 0 alone.
        for payload in [&b"abc"[..], b""] {
            let as_bytes = postcard::to_allocvec(&Bytes(payload)).unwrap();
            assert_eq!(as_bytes, postcard::to_allocvec(payload).unwrap());
            let read: Bytes<'_> = postcard::from_bytes(&as_bytes).unwrap();
            assert_eq!(read, Bytes(payload));
        }
        assert_eq!(postcard::to_allocvec(&Bytes(b"abc")).unwrap(), b"\x03abc");
        // Through a writer, as the log writes it.
        let mut out = Vec::new();
        postcard::to_io(&Bytes(b"xy"), &mut out).unwrap();
        assert_eq!(out, b"\x02xy");
    }
}
