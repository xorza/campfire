use campfire_common::{Tick, Ticks};
use serde::{Deserialize, Serialize};

/// The last tick a slot's inputs were scheduled to apply in, and how many apply there.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Spill {
    pub(super) tick: Tick,
    pub(super) count: u32,
}

impl Spill {
    /// The tick the next input that may apply from tick `earliest` on would apply in: the
    /// first, from there and from the last, that fewer than `max` of the slot's inputs fill.
    pub(super) const fn place(self, earliest: Tick, max: u32) -> Tick {
        if self.count == 0 || earliest.get() > self.tick.get() {
            earliest
        } else if self.count < max {
            self.tick
        } else {
            self.tick.after(Ticks::ONE)
        }
    }

    /// Schedules an input that may apply from tick `earliest` on, at its `place`.
    pub(super) const fn take(&mut self, earliest: Tick, max: u32) -> Tick {
        let tick = self.place(earliest, max);
        self.count = if self.count != 0 && tick.get() == self.tick.get() {
            self.count + 1
        } else {
            1
        };
        self.tick = tick;
        tick
    }
}
