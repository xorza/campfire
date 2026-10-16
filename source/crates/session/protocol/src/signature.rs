use serde::{Deserialize, Serialize};

/// A BIP-340 signature, `R.x ‖ s`: a session key's over a player's chain head or a connect
/// challenge, or the server key's over a server input, a checkpoint, a result or a receipt. Kept
/// as its two halves because serde encodes arrays of at most 32 elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature {
    r: [u8; 32],
    s: [u8; 32],
}

impl Signature {
    /// The bytes its encoding takes: the two halves, with no length.
    pub(crate) const ENCODED_LEN: u64 = 64;

    pub const fn from_bytes(bytes: [u8; 64]) -> Self {
        let (r, s) = bytes.split_at(32);
        let mut signature = Self {
            r: [0; 32],
            s: [0; 32],
        };
        signature.r.copy_from_slice(r);
        signature.s.copy_from_slice(s);
        signature
    }

    pub const fn to_bytes(self) -> [u8; 64] {
        let mut bytes = [0; 64];
        let (r, s) = bytes.split_at_mut(32);
        r.copy_from_slice(&self.r);
        s.copy_from_slice(&self.s);
        bytes
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::Binary;

    use super::*;

    #[test]
    fn a_signature_encodes_as_its_bytes_alone() {
        let signature = Signature::from_bytes([7; 64]);
        assert_eq!(Binary::encode(&signature), [7; 64]);
        assert_eq!(Signature::ENCODED_LEN, 64);
    }
}
