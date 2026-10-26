use campfire_log::{JsonText, LogEvent, LogLine};
use serde::Deserialize;
use tracing::warn;

use crate::error::JournalRefusal;

/// The server refused a player's journal command, for `refusal`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct JournalRefused {
    #[serde(deserialize_with = "LogLine::json")]
    pub refusal: JournalRefusal,
}

impl LogEvent for JournalRefused {
    const MESSAGE: &'static str = "refused a player's journal command";

    fn log(&self) {
        warn!(refusal = %JsonText(&self.refusal), "{}", Self::MESSAGE);
    }
}

#[cfg(test)]
mod tests;
