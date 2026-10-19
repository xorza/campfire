use std::fmt;
use std::hint;
use std::marker::PhantomData;

use serde::de::{Error, SeqAccess, Visitor};
use serde::ser::SerializeTuple;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use zeroize::{Zeroize, Zeroizing};

/// Bytes that stay secret, such as a seed before its segment is published: its `Debug` writes
/// `Secret(..)`, so no `{:?}` and no panic message shows them, and every type that holds secret
/// bytes holds them in one. It is not `Copy`, so each copy is a `clone` one can see, and each is
/// zeroed as it drops. Two compare in a time that does not depend on their bytes, and have no
/// order or hash, which would tell them. It serializes as `[u8; N]` does, a tuple of `N` bytes.
#[derive(Clone, Eq)]
pub struct Secret<const N: usize>([u8; N]);

impl<const N: usize> Secret<N> {
    pub const fn new(bytes: [u8; N]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; N] {
        &self.0
    }
}

/// Each byte's difference goes through `black_box`, so the compiler cannot stop at the first
/// one, as the `constant_time_eq` crate keeps a comparison from leaving early.
impl<const N: usize> PartialEq for Secret<N> {
    fn eq(&self, other: &Self) -> bool {
        let differ = (self.0.iter().zip(&other.0))
            .fold(0, |differ, (a, b)| hint::black_box(differ | (a ^ b)));
        differ == 0
    }
}

impl<const N: usize> Drop for Secret<N> {
    fn drop(&mut self) {
        self.0.zeroize();
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
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
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
        // Zeroed as it drops, so a read that fails part way leaves none of the secret behind.
        let mut bytes = Zeroizing::new([0; N]);
        for (at, byte) in bytes.iter_mut().enumerate() {
            *byte = seq
                .next_element()?
                .ok_or_else(|| A::Error::invalid_length(at, &self))?;
        }
        Ok(Secret(*bytes))
    }
}

#[cfg(test)]
mod tests {
    use std::array;

    use campfire_codec::Binary;

    use super::*;
    use crate::segment_seed::SegmentSeed;

    #[test]
    fn a_secret_prints_redacted_and_encodes_as_its_array() {
        let bytes: [u8; 32] = array::from_fn(|at| u8::try_from(at).unwrap() + 200);
        let seed = SegmentSeed::new(bytes);
        assert_eq!(format!("{seed:?}"), "SegmentSeed(Secret(..))");
        assert_eq!(format!("{:#?}", Secret::new([1_u8; 4])), "Secret(..)");
        // Two compare by every byte, the first and the last among them.
        assert_eq!(Secret::new([1_u8, 2, 3]), Secret::new([1, 2, 3]));
        assert_ne!(Secret::new([1_u8, 2, 3]), Secret::new([0, 2, 3]));
        assert_ne!(Secret::new([1_u8, 2, 3]), Secret::new([1, 2, 4]));
        // 32 bytes, each above 127, as postcard writes the array: one byte each, no length.
        let encoded = Binary::encode(&seed);
        assert_eq!(encoded, Binary::encode(&bytes));
        assert_eq!(encoded.len(), 32);
        let read: SegmentSeed = Binary::decode(&encoded).unwrap();
        assert_eq!(read.as_bytes(), &bytes);
        // One byte short does not read.
        assert!(Binary::decode::<SegmentSeed>(&encoded[..31]).is_err());
    }
}
