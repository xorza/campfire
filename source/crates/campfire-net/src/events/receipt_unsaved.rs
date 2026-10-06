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
mod tests {
    use campfire_log::internals::round_trip;

    use super::*;

    #[test]
    fn the_event_reads_back_what_it_logs() {
        round_trip(&ReceiptUnsaved {
            error: "No space left on device".to_owned(),
        });
    }
}
