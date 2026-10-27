#![expect(
    clippy::disallowed_methods,
    reason = "a derive of JsonSchema calls serde_json::to_value"
)]

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::session_status::GameSpeed;

/// A local match's pause and speed to set, each left as it is when none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PaceAsk {
    pub paused: Option<bool>,
    pub speed: Option<GameSpeed>,
}
