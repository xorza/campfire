use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::error;

/// The snapshot of the save a load goes back to does not read, or is not the one its checkpoint
/// names: the server exits before the load changes anything, as on a failed journal.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LoadFailed {
    pub error: String,
}

impl LogEvent for LoadFailed {
    const MESSAGE: &'static str = "the save's snapshot does not read; the server exits";

    fn log(&self) {
        error!(error = %self.error, "{}", Self::MESSAGE);
    }
}

#[cfg(test)]
mod tests;
