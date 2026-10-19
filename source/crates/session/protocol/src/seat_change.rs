use campfire_common::{AfterLeave, Seat, Tick};

/// A change of who controls a seat, as the log applied it, in the tick it applies in: what the
/// mode's rules check, and what its hooks hear.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeatChange {
    pub tick: Tick,
    pub seat: Seat,
    pub kind: SeatChangeKind,
}

/// What a seat change did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeatChangeKind {
    /// A player took the seat, which was `from`.
    Joined { from: Taken },
    /// Its player left, and it became `becomes`.
    Left { becomes: AfterLeave },
}

/// What a seat a player joined was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Taken {
    /// Open, and no seat the joiner left.
    Open,
    /// Played by a bot, and no seat the joiner left.
    Bot,
    /// The seat the joiner left, reserved, played by a bot or open.
    Own,
}
