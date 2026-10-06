use campfire_common::Tick;
use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::warn;

/// A checkpoint was due at the boundary before tick `tick`, past the seed chain's last segment:
/// the session ends there, aborted.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SeedsRanOut {
    pub tick: Tick,
}

impl LogEvent for SeedsRanOut {
    const MESSAGE: &'static str = "the seed chain ran out; the session ends aborted";

    fn log(&self) {
        warn!(tick = self.tick.get(), "{}", Self::MESSAGE);
    }
}

#[cfg(test)]
mod tests {
    use campfire_log::internals::round_trip;

    use super::*;

    #[test]
    fn the_event_reads_back_what_it_logs() {
        round_trip(&SeedsRanOut {
            tick: Tick::new(30),
        });
    }
}
