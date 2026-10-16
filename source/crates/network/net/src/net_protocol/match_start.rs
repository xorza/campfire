use campfire_common::{PlayerSlot, Tick};
use campfire_protocol::{Controller, InputHash, SessionLog};
use campfire_values::Team;
use serde::{Deserialize, Serialize};

/// Tells a client that it plays the match, as it starts or again after its link failed: which
/// player it is and its team, the Lightyear tick of sim tick `first`, and the player's chain:
/// the root of a new one, or the chain as the log holds it; and whether the session went back to
/// a save since it started, which may have dropped the chain's inputs after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct MatchStart {
    /// The Lightyear tick that is sim tick `first`.
    pub start_tick: u32,
    pub first: Tick,
    pub slot: PlayerSlot,
    pub team: Team,
    pub chain: SeatChain,
    /// A client of a local server then takes the server's chain as it holds it, whatever its own
    /// history.
    pub loaded: bool,
}

/// The chain a seated player signs: a new one, with no input yet, from `root`, the delegation's id
/// for a player the header seats and a root of its place for a join; or the one the log holds
/// for the slot's player, which goes on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum SeatChain {
    New { root: InputHash },
    Held(ChainHead),
}

/// Where a player's chain stands in the log: how many inputs it holds, the hash the next links
/// to, and the last stamp, below which the log refuses the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ChainHead {
    pub next_seq: u64,
    pub head: InputHash,
    pub last_stamp: Option<Tick>,
}

impl SeatChain {
    /// The new chain of `slot`'s player, who has sent no input yet, as `log` holds it.
    pub(crate) fn new_of(log: &SessionLog, slot: PlayerSlot) -> Self {
        let Some(Controller::Player { chain, .. }) = log.controller(slot) else {
            panic!("a player controls the slot seated");
        };
        debug_assert_eq!(chain.next_seq(), 0, "a new chain holds no input");
        Self::New { root: chain.head() }
    }
}
