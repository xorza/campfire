use campfire_log::{JsonText, LogEvent, LogLine};
use serde::Deserialize;
use tracing::warn;

use crate::error::ReceiptRefusal;

/// The client refused a receipt the server sent, for `refusal`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct ReceiptRefused {
    #[serde(deserialize_with = "LogLine::json")]
    pub refusal: ReceiptRefusal,
}

impl LogEvent for ReceiptRefused {
    const MESSAGE: &'static str = "refused a receipt from the server";

    fn log(&self) {
        warn!(refusal = %JsonText(&self.refusal), "{}", Self::MESSAGE);
    }
}

#[cfg(test)]
mod tests;
