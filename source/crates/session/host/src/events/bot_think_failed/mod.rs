use campfire_common::PlayerSlot;
use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::warn;

/// A think of the bot of `player_slot` failed, with why: it gives no order until it thinks again.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct BotThinkFailed {
    pub player_slot: PlayerSlot,
    pub error: String,
}

impl LogEvent for BotThinkFailed {
    const MESSAGE: &'static str = "a bot's think failed; it gives no order until its next think";

    fn log(&self) {
        warn!(player_slot = self.player_slot.get(), error = %self.error, "{}", Self::MESSAGE);
    }
}

#[cfg(test)]
mod tests;
