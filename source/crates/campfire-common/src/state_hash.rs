use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::bytes32::Bytes32;
use crate::bytes32::error::NotHex;

/// The hash of the whole simulated state, which the sim computes and the session log's
/// checkpoints and result carry. It writes, and reads back, as 64 lowercase hex digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StateHash(Bytes32);

impl StateHash {
    pub const fn new(bytes: [u8; 32]) -> StateHash {
        StateHash(Bytes32::new(bytes))
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}

impl fmt::Display for StateHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for StateHash {
    type Err = NotHex;

    fn from_str(text: &str) -> Result<StateHash, NotHex> {
        text.parse().map(StateHash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hash_writes_as_lowercase_hex_and_reads_back_from_that_alone() {
        // Its 32 bytes, two digits each, leading zeros kept.
        let mut bytes = [0; 32];
        bytes[0] = 0x0a;
        bytes[31] = 0xff;
        let written = StateHash::new(bytes).to_string();
        assert_eq!(written, format!("0a{}ff", "00".repeat(30)));
        assert_eq!(written.parse(), Ok(StateHash::new(bytes)));
        assert_eq!(written.to_uppercase().parse::<StateHash>(), Err(NotHex));
        let encoded = postcard::to_allocvec(&StateHash::new(bytes)).unwrap();
        assert_eq!(encoded, postcard::to_allocvec(&bytes).unwrap());
    }
}
