use campfire_capabilities::Team;
use campfire_common::{PlayerSlot, Tick};
use campfire_protocol::InputHash;
use serde::{Deserialize, Serialize};

/// Tells a client that it plays the match, as it starts or again after its link failed: which
/// player it is and its team, the Lightyear tick of sim tick `first`, and the player's chain as
/// the log holds it, or none for a chain that starts from their delegation's id; and whether the
/// session went back to a save since it started, which may have dropped the chain's inputs after
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct MatchStart {
    /// The Lightyear tick that is sim tick `first`.
    pub start_tick: u32,
    pub first: Tick,
    pub slot: PlayerSlot,
    pub team: Team,
    pub chain: Option<ChainHead>,
    /// A client of a local server then takes the server's chain as it holds it, whatever its own
    /// history.
    pub loaded: bool,
}

/// Where a player's chain stands in the log: how many inputs it holds, the hash the next links
/// to, and the last stamp, below which the log refuses the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ChainHead {
    pub next_seq: u64,
    pub head: InputHash,
    pub last_stamp: Option<Tick>,
}
