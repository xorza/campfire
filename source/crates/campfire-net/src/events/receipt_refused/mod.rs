use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::warn;

/// The client refused a receipt the server sent, for `reason`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ReceiptRefused {
    pub reason: String,
}

impl LogEvent for ReceiptRefused {
    const MESSAGE: &'static str = "refused a receipt from the server";

    fn log(&self) {
        warn!(reason = %self.reason, "{}", Self::MESSAGE);
    }
}

#[cfg(test)]
mod tests;
