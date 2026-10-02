use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::error;

/// The client's link to the server failed or ended before the client asked to leave; `reason` is
/// the transport's account of it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LinkLost {
    pub reason: String,
}

impl LogEvent for LinkLost {
    const MESSAGE: &'static str = "the link to the server failed or ended";

    fn log(&self) {
        error!(reason = %self.reason, "{}", Self::MESSAGE);
    }
}

#[cfg(test)]
mod tests;
