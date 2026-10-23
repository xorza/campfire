use campfire_common::PlayerSlot;
use serde::{Deserialize, Serialize};

/// A change of who controls a player slot that takes effect in the running tick, as the session log
/// applied it: the mode hears it in its stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerSlotEvent {
    pub player_slot: PlayerSlot,
    pub kind: PlayerSlotEventKind,
}

/// What a player slot event did to its player slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayerSlotEventKind {
    /// A player took the player slot.
    Joined,
    /// The player slot's player left it.
    Left,
}
