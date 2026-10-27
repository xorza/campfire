#![expect(
    clippy::disallowed_methods,
    reason = "a derive of JsonSchema calls serde_json::to_value"
)]

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::nearest_names::NearestNames;
use crate::request::RequestId;

/// Why a game answers no request, each case with what the request should change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConsoleRefusal {
    /// The line reads as no request of this build's types.
    Unread { error: String },
    /// The game holds no match yet.
    NoMatch,
    /// The match has no unit type of the name, and these are the names nearest it.
    UnknownType { name: String, nearest: NearestNames },
    /// The match has no unit of the stable id.
    UnknownUnit { id: u64 },
    /// A run or a pace is a local match's, and the game's match is not local.
    NotLocal,
    /// The run of the request of this id is under way; a cancel of it ends it.
    RunUnderWay { run: RequestId },
    /// A view is a client's, and the game has no window.
    NotClient,
    /// The window drew no frame in a second: it may be hidden, minimized or covered.
    NoFrame,
    /// A region of pixels is of the last screenshot, and there is none.
    NoShot,
    /// None of the units is on the screen.
    OffScreen,
}
