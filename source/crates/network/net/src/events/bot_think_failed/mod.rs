use campfire_common::Seat;
use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::warn;

/// A think of the bot of `seat` failed, with why: it gives no order until it thinks again.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct BotThinkFailed {
    pub seat: Seat,
    pub error: String,
}

impl LogEvent for BotThinkFailed {
    const MESSAGE: &'static str = "a bot's think failed; it gives no order until its next think";

    fn log(&self) {
        warn!(seat = self.seat.get(), error = %self.error, "{}", Self::MESSAGE);
    }
}

#[cfg(test)]
mod tests;
