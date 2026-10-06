use std::fmt;

use campfire_common::Bytes32;
use serde::{Deserialize, Serialize};

/// A delegation's id: the id of its Nostr event, which a player's first input links to and a
/// receipt names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DelegationId(Bytes32);

impl DelegationId {
    pub const fn new(bytes: [u8; 32]) -> DelegationId {
        DelegationId(Bytes32::new(bytes))
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}

/// In lowercase hex.
impl fmt::Display for DelegationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
