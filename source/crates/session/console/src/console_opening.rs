#![expect(
    clippy::disallowed_methods,
    reason = "a derive of JsonSchema calls serde_json::to_value"
)]

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::fingerprint::Fingerprint;

/// A console's first line: the game's role and the fingerprint of its console's types, which a
/// tool of other types refuses (console.md, A console opens with its fingerprint).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConsoleOpening {
    pub role: GameRole,
    pub fingerprint: Fingerprint,
}

/// What a game is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GameRole {
    Client,
    Server,
}
