use serde::{Deserialize, Serialize};

/// A player of the match, by seat: the place of the player in the session header, in join order.
/// The session log and the sim both name players by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Seat(u32);

impl Seat {
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    pub const fn get(self) -> u32 {
        self.0
    }

    /// The seat as an index into a list by seat.
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}
