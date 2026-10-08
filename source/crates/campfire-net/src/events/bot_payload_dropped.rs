use campfire_common::PlayerSlot;
use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::warn;

/// The server dropped an order of the bot of `slot` whose payload of `len` bytes passes the
/// session's max length.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BotPayloadDropped {
    pub slot: PlayerSlot,
    pub len: usize,
}

impl LogEvent for BotPayloadDropped {
    const MESSAGE: &'static str =
        "dropped a bot's order whose payload passes the session's max length";

    fn log(&self) {
        warn!(slot = self.slot.get(), len = self.len, "{}", Self::MESSAGE);
    }
}

#[cfg(test)]
mod tests {
    use campfire_log::internals::round_trip;

    use super::*;

    #[test]
    fn the_event_reads_back_what_it_logs() {
        round_trip(&BotPayloadDropped {
            slot: PlayerSlot::new(2),
            len: 4097,
        });
    }
}
