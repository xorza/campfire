use campfire_common::PlayerSlot;
use serde::{Deserialize, Serialize};

/// Tells a client that the match started, and which player it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct MatchStart {
    /// The Lightyear tick that is sim tick 0.
    pub start_tick: u32,
    pub slot: PlayerSlot,
}
