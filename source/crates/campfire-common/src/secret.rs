use std::fmt;
use std::marker::PhantomData;

use serde::de::{Error, SeqAccess, Visitor};
use serde::ser::SerializeTuple;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Bytes that stay secret, such as a seed before its segment is published: its `Debug` writes
/// `Secret(..)`, so no `{:?}` and no panic message shows them, and every type that holds secret
/// bytes holds them in one. It serializes as `[u8; N]` does, a tuple of `N` bytes.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Secret<const N: usize>([u8; N]);

impl<const N: usize> Secret<N> {
    pub const fn new(bytes: [u8; N]) -> Secret<N> {
        Secret(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; N] {
        &self.0
    }
}

impl<const N: usize> fmt::Debug for Secret<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(..)")
    }
}

impl<const N: usize> Serialize for Secret<N> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut tuple = serializer.serialize_tuple(N)?;
        for byte in &self.0 {
            tuple.serialize_element(byte)?;
        }
        tuple.end()
    }
}

impl<'de, const N: usize> Deserialize<'de> for Secret<N> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Secret<N>, D::Error> {
        deserializer.deserialize_tuple(N, SecretVisitor(PhantomData))
    }
}

/// Reads the `N` bytes of a `Secret<N>`.
#[derive(Debug)]
struct SecretVisitor<const N: usize>(PhantomData<[u8; N]>);

impl<'de, const N: usize> Visitor<'de> for SecretVisitor<N> {
    type Value = Secret<N>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{N} bytes")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Secret<N>, A::Error> {
        let mut bytes = [0; N];
        for (at, byte) in bytes.iter_mut().enumerate() {
            *byte = seq
                .next_element()?
                .ok_or_else(|| A::Error::invalid_length(at, &self))?;
        }
        Ok(Secret(bytes))
    }
}

#[cfg(test)]
mod tests {
    use std::array;

    use super::*;
    use crate::segment_seed::SegmentSeed;

    #[test]
    fn a_secret_prints_redacted_and_encodes_as_its_array() {
        let bytes: [u8; 32] = array::from_fn(|at| u8::try_from(at).unwrap() + 200);
        let seed = SegmentSeed::new(bytes);
        assert_eq!(format!("{seed:?}"), "SegmentSeed(Secret(..))");
        assert_eq!(format!("{:#?}", Secret::new([1_u8; 4])), "Secret(..)");
        // 32 bytes, each above 127, as postcard writes the array: one byte each, no length.
        let encoded = postcard::to_allocvec(&seed).unwrap();
        assert_eq!(encoded, postcard::to_allocvec(&bytes).unwrap());
        assert_eq!(encoded.len(), 32);
        let read: SegmentSeed = postcard::from_bytes(&encoded).unwrap();
        assert_eq!(read.as_bytes(), &bytes);
        // One byte short does not read.
        assert!(postcard::from_bytes::<SegmentSeed>(&encoded[..31]).is_err());
    }
}
