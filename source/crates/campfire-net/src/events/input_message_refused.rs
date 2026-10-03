use campfire_common::{PlayerSlot, Tick};
use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::warn;

/// The server refused an input message of `slot` as it was to log `next_tick`, which ends the
/// link; `error` says why.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct InputMessageRefused {
    pub slot: PlayerSlot,
    pub next_tick: Tick,
    pub error: String,
}

impl LogEvent for InputMessageRefused {
    const MESSAGE: &'static str = "refused an input message, which ends the link";

    fn log(&self) {
        warn!(
            slot = self.slot.get(),
            next_tick = self.next_tick.get(),
            error = %self.error,
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
        round_trip(&InputMessageRefused {
            slot: PlayerSlot::new(0),
            next_tick: Tick::new(42),
            error: "the chain does not follow its head".to_owned(),
        });
    }
}
