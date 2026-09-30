use serde::{Deserialize, Serialize};

/// A player's place in the session header, in join order.
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
