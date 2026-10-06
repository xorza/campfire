use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::error;

/// A write or a sync of the session's journal failed: the server keeps no record past it, so it
/// exits, and its host's supervisor starts it again, which restores the session.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct JournalFailed {
    pub error: String,
}

impl LogEvent for JournalFailed {
    const MESSAGE: &'static str = "the session's journal failed; the server exits";

    fn log(&self) {
        error!(error = %self.error, "{}", Self::MESSAGE);
    }
}

#[cfg(test)]
mod tests;
