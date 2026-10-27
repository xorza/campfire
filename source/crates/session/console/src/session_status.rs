#![expect(
    clippy::disallowed_methods,
    reason = "a derive of JsonSchema calls serde_json::to_value"
)]

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A session's status: its phase; its match's rate, in ticks a second, and state hash, as the log
/// writes it, once a match is open; the pace of a local match; each player slot's resources; and
/// the script calls that failed since the console opened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionStatus {
    pub phase: SessionPhase,
    pub rate: Option<u32>,
    pub state_hash: Option<String>,
    pub pace: Option<PaceStatus>,
    pub players: Vec<PlayerStatus>,
    pub script_failures: ScriptFailureCount,
}

/// Where a server's session is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionPhase {
    /// No session yet.
    Opening,
    /// The lobby is open, and players join it.
    Lobby,
    /// A session a stop ended waits to restore.
    Restoring,
    /// The match runs.
    Running,
    /// The match replays a published log.
    Replaying,
}

/// A local match's pause and speed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PaceStatus {
    pub paused: bool,
    pub speed: GameSpeed,
}

/// A local match's speed against its session's rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GameSpeed {
    Half,
    Normal,
    Double,
    Quadruple,
}

/// A player slot, counted from 0, and its resources, by the names the mode gives them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlayerStatus {
    pub player_slot: u32,
    pub resources: BTreeMap<String, i64>,
}

/// The count of the script calls that failed, and the latest.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ScriptFailureCount {
    pub count: u64,
    pub latest: Option<FailedScriptCall>,
}

/// A script call that failed: its tick, the unit it ran for, if any, by its stable id, its hook
/// and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FailedScriptCall {
    pub tick: u64,
    pub unit: Option<u64>,
    pub hook: String,
    pub error: String,
}

impl ScriptFailureCount {
    /// Counts `call`, the latest.
    pub fn note(&mut self, call: FailedScriptCall) {
        self.count += 1;
        self.latest = Some(call);
    }
}
