use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::warn;

/// A rollback asked for to `rollback_tick`, at the local `tick`, that lies after the local tick or
/// more than `max_rollback_ticks` before it, which no rollback restores, so none runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub(crate) struct RollbackRefused {
    pub(crate) rollback_tick: u32,
    pub(crate) tick: u32,
    pub(crate) max_rollback_ticks: u16,
}

impl LogEvent for RollbackRefused {
    const MESSAGE: &'static str = "a rollback past the window or ahead of the local tick runs none";

    fn log(&self) {
        warn!(
            rollback_tick = self.rollback_tick,
            tick = self.tick,
            max_rollback_ticks = self.max_rollback_ticks,
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
    fn the_event_reads_back_as_it_logs() {
        round_trip(&RollbackRefused {
            rollback_tick: 3,
            tick: 20,
            max_rollback_ticks: 16,
        });
    }
}
