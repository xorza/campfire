use campfire_common::PlayerSlot;
use serde::{Deserialize, Serialize};

/// A change of who controls a slot that takes effect in the running tick, as the session log
/// applied it: the mode hears it in its stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotEvent {
    pub slot: PlayerSlot,
    pub kind: SlotEventKind,
}

/// What a slot event did to its slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlotEventKind {
    /// A player took the slot.
    Joined,
    /// The slot's player left it.
    Left,
}
