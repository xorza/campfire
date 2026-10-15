use campfire_log::{JsonText, LogEvent, LogLine};
use serde::Deserialize;
use tracing::warn;

use crate::events::join_refused::error::JoinError;

/// The server refused the join of `link`, its entity's bits, as the lobby's other lines name it,
/// for `error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct JoinRefused {
    pub link: u64,
    #[serde(deserialize_with = "LogLine::json")]
    pub error: JoinError,
}

impl LogEvent for JoinRefused {
    const MESSAGE: &'static str = "refused a join";

    fn log(&self) {
        warn!(link = self.link, error = %JsonText(&self.error), "{}", Self::MESSAGE);
    }
}

pub(crate) mod error;

#[cfg(test)]
mod tests;
