use campfire_common::Tick;
use campfire_log::{JsonText, LogEvent, LogLine};
use campfire_mode::{MatchEnd, MatchResult};
use serde::Deserialize;
use tracing::info;

/// The match ended in `tick`, as `result` says. The server logs it after that tick, and a client
/// once the match's entity brings it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct MatchEnded {
    pub tick: Tick,
    #[serde(deserialize_with = "LogLine::json")]
    pub result: MatchResult,
}

impl MatchEnded {
    pub const fn of(end: MatchEnd) -> Self {
        Self {
            tick: end.tick(),
            result: end.result(),
        }
    }
}

impl LogEvent for MatchEnded {
    const MESSAGE: &'static str = "the match ended";

    fn log(&self) {
        info!(tick = self.tick.get(), result = %JsonText(&self.result), "{}", Self::MESSAGE);
    }
}

#[cfg(test)]
mod tests;
