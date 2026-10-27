use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::console_event::ConsoleEvent;
use crate::console_opening::{ConsoleOpening, GameRole};
use crate::fingerprint::Fingerprint;
use crate::reply::Reply;
use crate::request::Request;

/// A line a game writes to its console: its opening, first; a reply to a request; or an event of
/// its log, as it logs it (console.md, Events come over the console).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Output {
    Opening(ConsoleOpening),
    Reply(Reply),
    Event(ConsoleEvent),
}

impl Output {
    /// The fingerprint of this build's console: of its requests and its lines.
    pub fn fingerprint() -> Fingerprint {
        Fingerprint::of::<(Request, Self)>()
    }

    /// The opening line of a game of `role`, with this build's fingerprint.
    pub fn opening(role: GameRole) -> Self {
        Self::Opening(ConsoleOpening {
            role,
            fingerprint: Self::fingerprint(),
        })
    }
}
