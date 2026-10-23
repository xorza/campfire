use campfire_common::{PlayerSlot, Tick};
use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::warn;

/// The server logged an input of `player_slot`, stamped `stamp`, past the max input lead of
/// `next_tick`, the tick it was to log, so the input never takes effect
/// (05-protocol-spec.md, A late input applies at the next tick).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct InputNeverApplied {
    pub player_slot: PlayerSlot,
    pub stamp: Tick,
    pub next_tick: Tick,
}

impl LogEvent for InputNeverApplied {
    const MESSAGE: &'static str = "logged an input stamped past the max lead, which never applies";

    fn log(&self) {
        warn!(
            player_slot = self.player_slot.get(),
            stamp = self.stamp.get(),
            next_tick = self.next_tick.get(),
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
        round_trip(&InputNeverApplied {
            player_slot: PlayerSlot::new(1),
            stamp: Tick::new(60),
            next_tick: Tick::new(25),
        });
    }
}
