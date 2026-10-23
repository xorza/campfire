use campfire_common::PlayerSlot;

use crate::scripts::api_holder::ScriptHandle;

/// A player as a script holds it, `Player` in scripts: a player slot of the session, as only the
/// engine gives one, whose reads the modules that keep its parts register.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerHandle(PlayerSlot);

impl PlayerHandle {
    pub const fn new(player_slot: PlayerSlot) -> Self {
        Self(player_slot)
    }

    pub const fn player_slot(self) -> PlayerSlot {
        self.0
    }
}

impl ScriptHandle for PlayerHandle {
    const NAME: &'static str = "Player";
    const TITLE: &'static str = "Player";
}
