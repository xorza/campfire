use campfire_common::{PlayerSlot, Tick};
use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::debug;

/// The server logged an input of `slot`, stamped `stamp`, which takes effect in `tick`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct InputLogged {
    pub slot: PlayerSlot,
    pub stamp: Tick,
    pub tick: Tick,
}

impl LogEvent for InputLogged {
    const MESSAGE: &'static str = "logged an input";

    fn log(&self) {
        debug!(
            slot = self.slot.get(),
            stamp = self.stamp.get(),
            tick = self.tick.get(),
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
        round_trip(&InputLogged {
            slot: PlayerSlot::new(1),
            stamp: Tick::new(20),
            tick: Tick::new(21),
        });
    }
}
