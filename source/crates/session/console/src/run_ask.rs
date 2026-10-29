#![expect(
    clippy::disallowed_methods,
    reason = "a derive of JsonSchema calls serde_json::to_value"
)]

use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::units_ask::UnitActivity;

/// A run of a local match, at four times its session's rate, to where `to` says, or until
/// `wall_seconds` of the caller's time passed, 120 when it names none, then paused
/// (08-sessions.md, A console's local match starts paused).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RunAsk {
    pub to: RunTo,
    pub wall_seconds: Option<u32>,
}

/// Where a run ends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunTo {
    /// After this tick ran; a tick that ran already ends the run as it starts.
    Tick(u64),
    /// After this many ticks more, which the run makes a tick as it starts.
    Ticks(u64),
    /// After the first tick at whose end this holds.
    Until(RunCondition),
}

/// What ends a run after a tick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunCondition {
    /// The unit of this stable id is in this probe state.
    Unit { id: u64, state: UnitActivity },
    /// The game logged an event of this message.
    Event { message: String },
    /// A script call failed.
    ScriptFailure,
    /// The match ended.
    MatchEnd,
}

/// Why a run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunStop {
    /// It ran its tick.
    Reached,
    /// Its condition held.
    Held,
    /// Its wall time passed.
    WallTime,
    /// A cancel ended it.
    Cancelled,
    /// The match ended before its end.
    MatchEnded,
}

impl RunAsk {
    /// The wall time of a run that names none: under the two minutes Claude Code waits for a call
    /// before it sends the call to the background (08-sessions.md, A console's local match starts
    /// paused).
    pub const WALL: Duration = Duration::from_secs(110);

    /// The wall time it names, or `WALL`.
    pub fn wall(&self) -> Duration {
        self.wall_seconds.map_or(Self::WALL, |seconds| {
            Duration::from_secs(u64::from(seconds))
        })
    }
}
