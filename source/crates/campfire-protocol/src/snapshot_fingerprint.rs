use std::fmt;

use campfire_common::Bytes32;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A snapshot's identity: the SHA-256 of its bytes, as a Blossom server addresses a blob, so a
/// snapshot is shared as is. A checkpoint record names its snapshot by it, and the server names
/// the snapshot's file by it in hex.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SnapshotFingerprint(Bytes32);

impl SnapshotFingerprint {
    pub const fn new(bytes: [u8; 32]) -> SnapshotFingerprint {
        SnapshotFingerprint(Bytes32::new(bytes))
    }

    /// The fingerprint of the snapshot `bytes`.
    pub fn of(bytes: &[u8]) -> SnapshotFingerprint {
        SnapshotFingerprint::new(Sha256::digest(bytes).into())
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}

/// In lowercase hex.
impl fmt::Display for SnapshotFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fingerprint_is_the_sha_256_of_the_bytes() {
        // The SHA-256 of "abc", FIPS 180-2's first example.
        let expected = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(SnapshotFingerprint::of(b"abc").to_string(), expected);
    }
}
