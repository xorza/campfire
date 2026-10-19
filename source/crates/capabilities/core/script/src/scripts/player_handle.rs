use campfire_common::Seat;

use crate::scripts::api_holder::ScriptHandle;

/// A player as a script holds it, `Player` in scripts: a seat of the session, as only the engine
/// gives one, whose reads the modules that keep its parts register.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerHandle(Seat);

impl PlayerHandle {
    pub const fn new(seat: Seat) -> Self {
        Self(seat)
    }

    pub const fn seat(self) -> Seat {
        self.0
    }
}

impl ScriptHandle for PlayerHandle {
    const NAME: &'static str = "Player";
    const TITLE: &'static str = "Player";
}
