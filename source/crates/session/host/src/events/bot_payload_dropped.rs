use campfire_common::PlayerSlot;
use campfire_log::{JsonText, LogEvent, LogLine};
use campfire_runner::BotInputDrop;
use serde::Deserialize;
use tracing::warn;

/// The server dropped an input of the bot of `player_slot`, its payload `len` bytes long, for
/// `cause`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct BotPayloadDropped {
    pub player_slot: PlayerSlot,
    pub len: usize,
    #[serde(deserialize_with = "LogLine::json")]
    pub cause: BotInputDrop,
}

impl LogEvent for BotPayloadDropped {
    const MESSAGE: &'static str = "dropped a bot's input the session log refused";

    fn log(&self) {
        warn!(
            player_slot = self.player_slot.get(),
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
        for cause in [BotInputDrop::TooLarge, BotInputDrop::AheadOfTime] {
            round_trip(&BotPayloadDropped {
                player_slot: PlayerSlot::new(2),
                len: 4097,
                cause,
            });
        }
    }
}
