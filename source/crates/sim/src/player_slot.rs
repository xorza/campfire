use serde::{Deserialize, Serialize};

/// A player of the match, by slot: the place of the player in the session header, in join order.
/// The runner converts the session log's own slot type into it where the log's inputs enter the
/// sim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlayerSlot(u32);

impl PlayerSlot {
    pub const fn new(index: u32) -> PlayerSlot {
        PlayerSlot(index)
    }

    pub const fn get(self) -> u32 {
        self.0
    }

    /// The slot as an index into a list by slot.
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}
