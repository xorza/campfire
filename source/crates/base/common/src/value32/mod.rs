//! A 32-byte value of one meaning, a hash, an id or a key, as design 05's Hashes, ids and keys
//! names them.

use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

use crate::bytes32::Bytes32;
use crate::bytes32::error::NotHex;

/// What a `Value32` means: its marker, an empty enum that carries the meaning's name.
pub trait Meaning {
    /// The name its values go by, which their `Debug` writes.
    const NAME: &'static str;
}

/// A meaning whose value is the SHA-256 of bytes, as a Blossom server names a blob, so `of`
/// makes it.
pub trait ContentMeaning: Meaning {}

/// 32 bytes of the meaning `K`, every meaning's by one implementation, so each has the same calls:
/// it writes as exactly 64 lowercase hex digits, the one spelling it reads back, serializes as
/// its 32 bytes, and orders and hashes by them. Each meaning names it by an alias,
/// `pub type SessionId = Value32<SessionIdMeaning>`.
pub struct Value32<K> {
    bytes: Bytes32,
    meaning: PhantomData<fn() -> K>,
}

impl<K> Value32<K> {
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self {
            bytes: Bytes32::new(bytes),
            meaning: PhantomData,
        }
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        self.bytes.as_bytes()
    }

    pub const fn get(self) -> [u8; 32] {
        self.bytes.get()
    }

    /// The value the 64 lowercase hex digits `hex` write, as a literal in the code gives it; a
    /// text from outside reads by `FromStr`, which refuses a flawed one.
    pub const fn of_hex(hex: &str) -> Self {
        Self::new(Bytes32::of_hex(hex).get())
    }
}

impl<K: ContentMeaning> Value32<K> {
    /// The SHA-256 of `bytes`.
    pub fn of(bytes: &[u8]) -> Self {
        Self::new(Sha256::digest(bytes).into())
    }
}

impl<K> Clone for Value32<K> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<K> Copy for Value32<K> {}

impl<K> PartialEq for Value32<K> {
    fn eq(&self, other: &Self) -> bool {
        self.bytes == other.bytes
    }
}

impl<K> Eq for Value32<K> {}

impl<K> PartialOrd for Value32<K> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<K> Ord for Value32<K> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.bytes.cmp(&other.bytes)
    }
}

impl<K> Hash for Value32<K> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.bytes.hash(state);
    }
}

/// Its meaning's name and its hex, `SessionId(ab…05)`.
impl<K: Meaning> fmt::Debug for Value32<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({})", K::NAME, self.bytes)
    }
}

impl<K> fmt::Display for Value32<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.bytes, f)
    }
}

impl<K> FromStr for Value32<K> {
    type Err = NotHex;

    fn from_str(hex: &str) -> Result<Self, NotHex> {
        hex.parse::<Bytes32>().map(|bytes| Self::new(bytes.get()))
    }
}

impl<K> Serialize for Value32<K> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.bytes.serialize(serializer)
    }
}

impl<'de, K> Deserialize<'de> for Value32<K> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Bytes32::deserialize(deserializer).map(|bytes| Self::new(bytes.get()))
    }
}

#[cfg(test)]
mod tests;
