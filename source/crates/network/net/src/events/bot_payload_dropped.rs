use campfire_common::Seat;
use campfire_log::{JsonText, LogEvent, LogLine};
use serde::{Deserialize, Serialize};
use tracing::warn;

/// The server dropped an input of the bot of `seat`, its payload `len` bytes long, for `cause`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct BotPayloadDropped {
    pub seat: Seat,
    pub len: usize,
    #[serde(deserialize_with = "LogLine::json")]
    pub cause: DropCause,
}

/// Why the log refused a bot's input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DropCause {
    /// The payload passes the session's max length.
    TooLarge,
    /// The seat's inputs fill every tick up to the session's max input lead.
    AheadOfTime,
    /// The session log is full: the session ends at the next boundary.
    LogFull,
}

impl LogEvent for BotPayloadDropped {
    const MESSAGE: &'static str = "dropped a bot's input the session log refused";

    fn log(&self) {
        warn!(
            seat = self.seat.get(),
            len = self.len,
            cause = %JsonText(&self.cause),
            "{}",
            Self::MESSAGE
        );
    }
}

#[cfg(test)]
mod tests {
    use campfire_log::internals::round_trip;

    use super::*;

    #[test]
    fn the_event_reads_back_what_it_logs() {
        for cause in [DropCause::TooLarge, DropCause::AheadOfTime] {
            round_trip(&BotPayloadDropped {
                seat: Seat::new(2),
                len: 4097,
                cause,
            });
        }
    }
}
