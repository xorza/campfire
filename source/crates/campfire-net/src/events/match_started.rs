use campfire_common::PlayerSlot;
use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::info;

/// A client's match started: the server gave it `slot`, and sim tick 0 is Lightyear's
/// `start_tick`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct MatchStarted {
    pub slot: PlayerSlot,
    pub start_tick: u32,
}

impl LogEvent for MatchStarted {
    const MESSAGE: &'static str = "the match started";

    fn log(&self) {
        info!(
            slot = self.slot.get(),
            start_tick = self.start_tick,
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
        round_trip(&MatchStarted {
            slot: PlayerSlot::new(1),
            start_tick: 5,
        });
    }
}
