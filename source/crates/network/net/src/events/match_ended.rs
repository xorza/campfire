use campfire_common::Tick;
use campfire_log::{JsonText, LogEvent, LogLine};
use campfire_mode::{MatchEnd, MatchResult};
use serde::Deserialize;
use tracing::info;

/// The match ended in `tick`, as `result` says. The server logs it after that tick, and a client
/// once the match's entity brings it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub(crate) struct MatchEnded {
    pub tick: Tick,
    #[serde(deserialize_with = "LogLine::json")]
    pub result: MatchResult,
}

impl MatchEnded {
    pub(crate) const fn of(end: MatchEnd) -> MatchEnded {
        MatchEnded {
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
mod tests {
    use campfire_log::internals::round_trip;
    use campfire_values::Team;

    use super::*;

    #[test]
    fn the_event_reads_back_what_it_logs() {
        for result in [MatchResult::Won(Team::new(1)), MatchResult::Draw] {
            let end = MatchEnd::new(Tick::new(900), result);
            assert_eq!(
                MatchEnded::of(end),
                MatchEnded {
                    tick: Tick::new(900),
                    result,
                }
            );
            round_trip(&MatchEnded::of(end));
        }
    }
}
