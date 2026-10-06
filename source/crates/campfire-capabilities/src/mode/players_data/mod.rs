use serde::Deserialize;

/// The mode's `[players]`: who may take a slot once the match started, beside the players it
/// started with. A player who left keeps the right to take their slot back while it is reserved,
/// played by a bot or still open; the server's door and the log's records follow these rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PlayersData {
    /// A new player may take an open slot.
    pub late_join: bool,
    /// A new player may take a slot a bot plays; only with `late_join`, which the load checks.
    pub bot_takeover: bool,
    /// What a slot becomes when its player leaves.
    pub leaver: Leaver,
}

impl PlayersData {
    /// What a mode with no `[players]` says: no late join, no bot takeover, and a leaver's slot
    /// reserved.
    pub const DEFAULT: PlayersData = PlayersData {
        late_join: false,
        bot_takeover: false,
        leaver: Leaver::Reserve,
    };
}

/// What the slot of a player who left becomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Leaver {
    /// It stays the leaver's alone, and idle.
    #[default]
    Reserve,
    /// A bot plays it.
    Bot,
    /// It is open, for a new player when `late_join` is on.
    Open,
}

#[cfg(test)]
mod tests;
