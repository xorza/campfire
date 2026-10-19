use serde::{Deserialize, Serialize};

/// What the seat of a player who left becomes, as the mode's `[players] leaver` says, and as
/// the log records each leave.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AfterLeave {
    /// It stays the leaver's alone, and idle.
    #[default]
    Reserve,
    /// A bot plays it.
    Bot,
    /// It is open, for a new player when `late_join` is on.
    Open,
}
