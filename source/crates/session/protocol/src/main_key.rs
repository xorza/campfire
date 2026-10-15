use campfire_common::Bytes32;
use derive_more::Display;
use serde::{Deserialize, Serialize};

/// A player's main key: the x-only public key of their Nostr identity, which signs their
/// delegations, as its 32 bytes. It writes as 64 lowercase hex digits.
#[derive(
    Debug, Display, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct MainKey(Bytes32);

impl MainKey {
    pub const fn new(bytes: [u8; 32]) -> MainKey {
        MainKey(Bytes32::new(bytes))
    }
}
