use serde::{Deserialize, Serialize};

/// The BLAKE3 hash that links a player's input to the one before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InputHash([u8; 32]);

impl InputHash {
    pub const fn new(bytes: [u8; 32]) -> InputHash {
        InputHash(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}
