use derive_more::Display;
use serde::{Deserialize, Serialize};

use crate::bytes32::Bytes32;

/// A package's identity: the SHA-256 of its index, `package.index`, which lists one `(path,
/// size, SHA-256)` row per file, sorted by path bytes, after its format's tag. Any change to any
/// file, or to the list, gives another fingerprint.
#[derive(
    Debug, Display, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Fingerprint(Bytes32);

impl Fingerprint {
    pub const fn new(bytes: [u8; 32]) -> Fingerprint {
        Fingerprint(Bytes32::new(bytes))
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}

#[cfg(test)]
mod tests {
    use crate::codec::binary::Binary;

    use super::*;

    #[test]
    fn a_fingerprint_shows_as_lowercase_hex_and_encodes_as_its_bytes() {
        let mut bytes = [0; 32];
        bytes[0] = 0xAB;
        bytes[31] = 0x05;
        let fingerprint = Fingerprint::new(bytes);
        assert_eq!(fingerprint.to_string(), format!("ab{}05", "00".repeat(30)));
        let encoded = Binary::encode(&fingerprint);
        assert_eq!(encoded, Binary::encode(&bytes));
    }
}
