use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::error;

/// The snapshot of a checkpoint was not written: the server exits, as on a failed journal, and
/// its host's supervisor starts it again, which takes the checkpoint again.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CheckpointFailed {
    pub error: String,
}

impl LogEvent for CheckpointFailed {
    const MESSAGE: &'static str = "a checkpoint's snapshot was not written; the server exits";

    fn log(&self) {
        error!(error = %self.error, "{}", Self::MESSAGE);
    }
}

#[cfg(test)]
mod tests;
