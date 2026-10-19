use campfire_common::{Seat, Tick};
use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::debug;

/// The server logged the input of `seat` at chain seq `seq`, stamped `stamp`, which takes effect
/// in `tick`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct InputLogged {
    pub seat: Seat,
    pub seq: u64,
    pub stamp: Tick,
    pub tick: Tick,
}

impl LogEvent for InputLogged {
    const MESSAGE: &'static str = "logged an input";

    fn log(&self) {
        debug!(
            seat = self.seat.get(),
            seq = self.seq,
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
            seat: Seat::new(1),
            seq: 7,
            stamp: Tick::new(20),
            tick: Tick::new(21),
        });
    }
}
