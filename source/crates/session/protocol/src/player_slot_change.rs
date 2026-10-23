use campfire_common::{AfterLeave, PlayerSlot, Tick};

/// A change of who controls a player slot, as the log applied it, in the tick it applies in: what
/// the mode's rules check, and what its hooks hear.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerSlotChange {
    pub tick: Tick,
    pub player_slot: PlayerSlot,
    pub kind: PlayerSlotChangeKind,
}

/// What a player slot change did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerSlotChangeKind {
    /// A player took the player slot, which was `from`.
    Joined { from: Taken },
    /// Its player left, and it became `becomes`.
    Left { becomes: AfterLeave },
}

/// What a player slot a player joined was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Taken {
    /// Open, and no player slot the joiner left.
    Open,
    /// Played by a bot, and no player slot the joiner left.
    Bot,
    /// The player slot the joiner left, reserved, played by a bot or open.
    Own,
}
