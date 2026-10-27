#![expect(
    clippy::disallowed_methods,
    reason = "a derive of JsonSchema calls serde_json::to_value"
)]

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::camera_ask::CameraAsk;
use crate::pace_ask::PaceAsk;
use crate::run_ask::RunAsk;
use crate::shot_ask::ShotAsk;
use crate::units_ask::UnitsAsk;

/// A line a tool writes to a game's console: what it asks, under an id its reply carries, so
/// replies come in any order (02-engine-core.md, A request carries its id, and a reply its tick).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Request {
    pub id: RequestId,
    pub ask: ConsoleAsk,
}

/// A request's id, the tool's own, unique among its requests to one game.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct RequestId(pub u64);

/// What a request asks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConsoleAsk {
    /// The session's status.
    Status,
    /// A table of the match's units.
    Units(UnitsAsk),
    /// A run of a local match, which pauses as it ends; its reply comes as it ends.
    Run(RunAsk),
    /// The end of the run the request of this id started, at the tick it reached.
    Cancel(RequestId),
    /// A local match's pause or speed, each as it was when the request names none.
    Pace(PaceAsk),
    /// A screenshot of a client's window.
    Screenshot(ShotAsk),
    /// A read or a move of a client's camera.
    Camera(CameraAsk),
}
