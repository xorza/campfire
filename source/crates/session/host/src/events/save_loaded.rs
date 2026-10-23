use campfire_common::Tick;
use campfire_log::{LogEvent, LogLine};
use campfire_protocol::SessionId;
use serde::Deserialize;
use tracing::info;

/// The server's session went back to a save in place: its match runs on from sim tick `tick`, and
/// its clients join again on their links.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SaveLoaded {
    #[serde(deserialize_with = "LogLine::text")]
    pub session: SessionId,
    pub tick: Tick,
}

impl LogEvent for SaveLoaded {
    const MESSAGE: &'static str = "went back to the save; the match runs on from it";

    fn log(&self) {
        info!(session = %self.session, tick = self.tick.get(), "{}", Self::MESSAGE);
    }
}

#[cfg(test)]
mod tests {
    use campfire_log::internals::round_trip;

    use super::*;

    #[test]
    fn the_event_reads_back_what_it_logs() {
        round_trip(&SaveLoaded {
            session: SessionId::new([1; 32]),
            tick: Tick::new(120),
        });
    }
}
