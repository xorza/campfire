use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::warn;

/// The client kept a receipt, and could not write it to its data directory: `error` says why.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ReceiptUnsaved {
    pub error: String,
}

impl LogEvent for ReceiptUnsaved {
    const MESSAGE: &'static str = "could not write a receipt to the data directory";

    fn log(&self) {
        warn!(error = %self.error, "{}", Self::MESSAGE);
    }
}

#[cfg(test)]
mod tests;
