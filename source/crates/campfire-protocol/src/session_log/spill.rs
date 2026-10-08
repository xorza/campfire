use serde::{Deserialize, Serialize};

use campfire_common::{Tick, Ticks};

/// The last tick a slot's inputs were scheduled to apply in, and how many apply there.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Spill {
    pub(super) tick: Tick,
    pub(super) count: u32,
}

impl Spill {
    /// The tick an input that may apply from tick `earliest` on applies in: the first, from
    /// there and from the last, that fewer than `max` of the slot's inputs fill.
    pub(super) const fn take(&mut self, earliest: Tick, max: u32) -> Tick {
        if self.count == 0 || earliest.get() > self.tick.get() {
            self.tick = earliest;
            self.count = 1;
        } else if self.count < max {
            self.count += 1;
        } else {
            self.tick = self.tick.after(Ticks::ONE);
            self.count = 1;
        }
        self.tick
    }

    /// How many of the slot's inputs apply in `tick` so far.
    pub(super) const fn at(self, tick: Tick) -> u32 {
        if self.tick.get() == tick.get() {
            self.count
        } else {
            0
        }
    }
}
