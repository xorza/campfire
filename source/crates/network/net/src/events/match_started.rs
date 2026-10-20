use campfire_common::Seat;
use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::info;

/// A client's match started: the server gave it `seat`, and sim tick 0 is the network stack's
/// `start_tick`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct MatchStarted {
    pub seat: Seat,
    pub start_tick: u32,
}

impl LogEvent for MatchStarted {
    const MESSAGE: &'static str = "the match started";

    fn log(&self) {
        info!(
            seat = self.seat.get(),
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
            seat: Seat::new(1),
            start_tick: 5,
        });
    }
}
