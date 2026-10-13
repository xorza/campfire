use campfire_common::Tick;
use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::trace;

/// A server frame starts, before it runs sim tick `tick`, its next, if it runs any: at Trace, as
/// a log line a frame is for a check that times each frame, not for every log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct ServerFrame {
    pub tick: Tick,
}

impl LogEvent for ServerFrame {
    const MESSAGE: &'static str = "a server frame starts";

    fn log(&self) {
        trace!(tick = self.tick.get(), "{}", Self::MESSAGE);
    }
}

#[cfg(test)]
mod tests {
    use campfire_log::internals::round_trip;

    use super::*;

    #[test]
    fn the_event_reads_back_what_it_logs() {
        round_trip(&ServerFrame {
            tick: Tick::new(20),
        });
    }
}
