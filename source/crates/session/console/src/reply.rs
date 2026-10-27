#![expect(
    clippy::disallowed_methods,
    reason = "a derive of JsonSchema calls serde_json::to_value"
)]

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::camera_ask::CameraPose;
use crate::console_refusal::ConsoleRefusal;
use crate::request::RequestId;
use crate::run_ask::RunStop;
use crate::session_status::{PaceStatus, SessionStatus};
use crate::shot::WindowShot;
use crate::unit_table::UnitTable;

/// A game's answer to the request `id`, and the tick its world held as it read, the last tick
/// that ended: none before the first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Reply {
    pub id: RequestId,
    pub tick: Option<u64>,
    pub answer: ConsoleAnswer,
}

/// What a game answers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConsoleAnswer {
    Status(SessionStatus),
    Units(UnitTable),
    /// The run ended, paused, for this reason, at the reply's tick.
    Ran(RunStop),
    /// A cancel was taken; the run it names ends with a reply of its own.
    Cancelled,
    /// The pace as it now stands.
    Paced(PaceStatus),
    Screenshot(WindowShot),
    /// The camera's rig as it now stands.
    Camera(CameraPose),
    /// The request asks what the game cannot give, and says what to change.
    Refused(ConsoleRefusal),
}
