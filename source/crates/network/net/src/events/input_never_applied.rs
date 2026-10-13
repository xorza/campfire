use campfire_common::{PlayerSlot, Tick};
use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::warn;

/// The server logged an input of `slot`, stamped `stamp`, as it was to log `next_tick`, and the
/// input never takes effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct InputNeverApplied {
    pub slot: PlayerSlot,
    pub stamp: Tick,
    pub next_tick: Tick,
    pub outcome: Unapplied,
}

/// Why a logged input never takes effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Unapplied {
    /// It arrived more than the max input delay after its stamp.
    Late,
    /// Its stamp was more than the max input lead ahead of the next tick.
    Early,
}

impl LogEvent for InputNeverApplied {
    const MESSAGE: &'static str = "logged an input that never takes effect";

    fn log(&self) {
        warn!(
            slot = self.slot.get(),
            stamp = self.stamp.get(),
            next_tick = self.next_tick.get(),
            outcome = ?self.outcome,
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
        for outcome in [Unapplied::Late, Unapplied::Early] {
            round_trip(&InputNeverApplied {
                slot: PlayerSlot::new(1),
                stamp: Tick::new(20),
                next_tick: Tick::new(25),
                outcome,
            });
        }
    }
}
