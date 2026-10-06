use campfire_common::Bytes32;
use derive_more::Display;
use serde::{Deserialize, Serialize};

/// The BLAKE3 hash that links a player's input to the one before it.
#[derive(
    Debug, Display, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct InputHash(Bytes32);

impl InputHash {
    pub const fn new(bytes: [u8; 32]) -> InputHash {
        InputHash(Bytes32::new(bytes))
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}
