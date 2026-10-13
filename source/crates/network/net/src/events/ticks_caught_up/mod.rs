use campfire_common::Tick;
use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::debug;

/// The server ran the ticks from `first` to `last`, more than one, in one frame, as it caught up
/// with its clock after a stall. It reads inputs once a frame, before the frame's ticks, so an
/// input stamped within them that it read only after them takes effect in the tick after `last`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct TicksCaughtUp {
    pub first: Tick,
    pub last: Tick,
}

impl TicksCaughtUp {
    /// The ticks of a frame that started with `next` the next tick and ended with `after`; none
    /// for a frame that ran one tick or none.
    pub const fn of(next: Tick, after: Tick) -> Option<TicksCaughtUp> {
        if after.get() <= next.get() + 1 {
            return None;
        }
        Some(TicksCaughtUp {
            first: next,
            last: Tick::new(after.get() - 1),
        })
    }

    /// Whether an input stamped `stamp` that took effect in `tick` did so only because it waited
    /// for these ticks.
    pub const fn delayed(&self, stamp: Tick, tick: Tick) -> bool {
        self.first.get() <= stamp.get()
            && stamp.get() <= self.last.get()
            && tick.get() == self.last.get() + 1
    }
}

impl LogEvent for TicksCaughtUp {
    const MESSAGE: &'static str = "ran several ticks in one frame";

    fn log(&self) {
        debug!(
            first = self.first.get(),
            last = self.last.get(),
            "{}",
            Self::MESSAGE
        );
    }
}

#[cfg(test)]
mod tests;
