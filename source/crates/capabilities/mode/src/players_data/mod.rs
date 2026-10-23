use campfire_common::AfterLeave;
use campfire_script::{DataSection, DataTable};
use serde::{Deserialize, Serialize};

use crate::player_control::PlayerControl;

/// The mode's `[players]`: who may take a slot once the match started, beside the players it
/// started with, and how a player controls its units. A player who left keeps the right to take
/// their slot back while it is reserved, played by a bot or still open; the server's door and the
/// log's records follow these rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PlayersData {
    /// A new player may take an open player slot.
    pub late_join: bool,
    /// A new player may take a slot a bot plays; only with `late_join`, which the load checks.
    pub bot_takeover: bool,
    /// What a slot becomes when its player leaves.
    pub leaver: AfterLeave,
    /// How a player controls its units; `avatar` only in a mode of avatars, which the load
    /// checks.
    pub control: PlayerControl,
}

impl PlayersData {
    /// What a mode with no `[players]` says: no late join, no bot takeover, a leaver's slot
    /// reserved, and a selection of the player's units.
    pub const DEFAULT: Self = Self {
        late_join: false,
        bot_takeover: false,
        leaver: AfterLeave::Reserve,
        control: PlayerControl::Units,
    };
}

impl Default for PlayersData {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl DataSection for PlayersData {
    const TABLE: DataTable = DataTable::new("The mode's `[players]`");
}

#[cfg(test)]
mod tests;
