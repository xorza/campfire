use campfire_common::{PlayerSlot, Tick};
use campfire_protocol::{Controller, InputHash, SessionLog};
use campfire_values::Team;
use serde::{Deserialize, Serialize};

use crate::match_clock::MatchClock;

/// Tells a client that it plays the match, as it starts or again after its link failed: which
/// player it is and its team, the clock of the match from the tick it plays on, and the player's
/// chain:
/// the root of a new one, or the chain as the log holds it; and whether the session went back to
/// a save since it started, which may have dropped the chain's inputs after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchStart {
    pub clock: MatchClock,
    pub player_slot: PlayerSlot,
    pub team: Team,
    pub chain: PlayerSlotChain,
    /// A client of a local server then takes the server's chain as it holds it, whatever its own
    /// history.
    pub loaded: bool,
}

/// The chain a placed player signs: a new one, with no input yet, from `root`, the delegation's id
/// for a player the header player slots and a root of its place for a join; or the one the log
/// holds for the player slot's player, which goes on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayerSlotChain {
    New { root: InputHash },
    Held(ChainHead),
}

/// Where a player's chain stands in the log: how many inputs it holds, the hash the next links
/// to, and the last stamp, below which the log refuses the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainHead {
    pub next_seq: u64,
    pub head: InputHash,
    pub last_stamp: Option<Tick>,
}

impl PlayerSlotChain {
    /// The new chain of `player_slot`'s player, who has sent no input yet, as `log` holds it.
    pub fn new_of(log: &SessionLog, player_slot: PlayerSlot) -> Self {
        let Some(Controller::Player { chain, .. }) = log.controller(player_slot) else {
            panic!("a player controls the player slot placed");
        };
        debug_assert_eq!(chain.next_seq(), 0, "a new chain holds no input");
        Self::New { root: chain.head() }
    }
}
