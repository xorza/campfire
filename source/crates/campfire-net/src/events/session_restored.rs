use campfire_common::Tick;
use campfire_log::{LogEvent, LogLine};
use campfire_protocol::SessionId;
use serde::Deserialize;
use tracing::info;

/// The server started again and restored the session a stop ended: it replayed the log, and runs
/// the match on from sim tick `tick`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SessionRestored {
    #[serde(deserialize_with = "LogLine::text")]
    pub session: SessionId,
    pub tick: Tick,
}

impl LogEvent for SessionRestored {
    const MESSAGE: &'static str = "restored the session; its match runs on";

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
        round_trip(&SessionRestored {
            session: SessionId::new([1; 32]),
            tick: Tick::new(120),
        });
    }
}
