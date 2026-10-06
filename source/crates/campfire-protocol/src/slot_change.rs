use campfire_common::{PlayerSlot, Tick};

use crate::server_input::AfterLeave;

/// A change of who controls a slot, as the log applied it, in the tick it applies in: what the
/// mode's rules check, and what its hooks hear.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotChange {
    pub tick: Tick,
    pub slot: PlayerSlot,
    pub kind: SlotChangeKind,
}

/// What a slot change did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotChangeKind {
    /// A player took the slot, which was `from`.
    Joined { from: Taken },
    /// Its player left, and it became `becomes`.
    Left { becomes: AfterLeave },
}

/// What a slot a player joined was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Taken {
    /// Open, and no slot the joiner left.
    Open,
    /// Played by a bot, and no slot the joiner left.
    Bot,
    /// The slot the joiner left, reserved, played by a bot or open.
    Own,
}
