use campfire_common::{PlayerSlot, Tick};
use campfire_protocol::InputHash;
use serde::{Deserialize, Serialize};

/// Tells a client that it plays the match, as it starts or again after its link failed: which
/// player it is, the Lightyear tick of sim tick `first`, and the player's chain as the log holds
/// it, or none for a chain that starts from their delegation's id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct MatchStart {
    /// The Lightyear tick that is sim tick `first`.
    pub start_tick: u32,
    pub first: Tick,
    pub slot: PlayerSlot,
    pub chain: Option<ChainHead>,
}

/// Where a player's chain stands in the log: how many inputs it holds, and the hash the next
/// links to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ChainHead {
    pub next_seq: u64,
    pub head: InputHash,
}
