use campfire_common::Seat;
use serde::{Deserialize, Serialize};

/// A change of who controls a seat that takes effect in the running tick, as the session log
/// applied it: the mode hears it in its stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatEvent {
    pub seat: Seat,
    pub kind: SeatEventKind,
}

/// What a seat event did to its seat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SeatEventKind {
    /// A player took the seat.
    Joined,
    /// The seat's player left it.
    Left,
}
