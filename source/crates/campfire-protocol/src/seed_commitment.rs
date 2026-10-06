use campfire_common::Bytes32;
use derive_more::Display;
use serde::{Deserialize, Serialize};

/// `C(s_0)`, the hash of the first segment's server seed. The terms hold it, so the session id
/// binds the server to every segment's seed before any player joins.
#[derive(
    Debug, Display, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct SeedCommitment(Bytes32);

impl SeedCommitment {
    pub const fn new(bytes: [u8; 32]) -> SeedCommitment {
        SeedCommitment(Bytes32::new(bytes))
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}
