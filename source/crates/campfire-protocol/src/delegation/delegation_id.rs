use campfire_common::Bytes32;
use derive_more::Display;
use serde::{Deserialize, Serialize};

/// A delegation's id: the id of its Nostr event, which a player's first input links to and a
/// receipt names.
#[derive(
    Debug, Display, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
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
