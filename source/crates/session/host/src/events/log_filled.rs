use campfire_common::Tick;
use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::warn;

/// The session log was full at the boundary before tick `tick`: its file or its journal kept
/// only the room the session's end takes, so the session ends there.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LogFilled {
    pub tick: Tick,
}

impl LogEvent for LogFilled {
    const MESSAGE: &'static str = "the session log is full; the session ends";

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
        round_trip(&LogFilled {
            tick: Tick::new(30),
        });
    }
}
