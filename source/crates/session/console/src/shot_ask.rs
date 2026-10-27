#![expect(
    clippy::disallowed_methods,
    reason = "a derive of JsonSchema calls serde_json::to_value"
)]

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A screenshot of the client's window, of `region` (02-engine-core.md, A pixel is the
/// screenshot's).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ShotAsk {
    pub region: ShotRegion,
}

/// The part of the window a screenshot takes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ShotRegion {
    /// The whole window.
    Window,
    /// A rectangle of the last screenshot's pixels, its top left at `x`, `y`.
    Pixels {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
    /// The box round the units of these stable ids on the screen.
    Units { ids: Vec<u64> },
}
