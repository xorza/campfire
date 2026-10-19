use campfire_common::Seat;
use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::warn;

/// The server refused an input message of `seat` whose frames do not fit its payloads, which
/// ends the link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub(crate) struct InputMessageUnfit {
    pub seat: Seat,
}

impl LogEvent for InputMessageUnfit {
    const MESSAGE: &'static str = "refused an input message whose frames do not fit its payloads";

    fn log(&self) {
        warn!(seat = self.seat.get(), "{}", Self::MESSAGE);
    }
}

#[cfg(test)]
mod tests {
    use campfire_log::internals::round_trip;

    use super::*;

    #[test]
    fn the_event_reads_back_what_it_logs() {
        round_trip(&InputMessageUnfit { seat: Seat::new(1) });
    }
}
