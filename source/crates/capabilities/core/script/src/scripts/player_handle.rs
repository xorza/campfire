use campfire_common::PlayerSlot;

use crate::scripts::api_owner::ScriptHandle;

/// A player as a script holds it, `Player` in scripts: a slot of the session, as only the engine
/// gives one, whose reads the modules that keep its parts register.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerHandle(PlayerSlot);

impl PlayerHandle {
    pub const fn new(slot: PlayerSlot) -> Self {
        Self(slot)
    }

    pub const fn slot(self) -> PlayerSlot {
        self.0
    }
}

impl ScriptHandle for PlayerHandle {
    const NAME: &'static str = "Player";
    const TITLE: &'static str = "Player";
}
