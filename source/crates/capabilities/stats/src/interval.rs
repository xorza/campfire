use campfire_common::{Tick, Ticks};
use serde::{Deserialize, Serialize};

/// A modifier's interval: every `every` ticks, the next in tick `next`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Interval {
    pub every: Ticks,
    pub next: Tick,
}

impl Interval {
    /// The first of its ticks after `now`: `next`, or that past every tick of it `now` passed; an
    /// interval of no ticks stays at `next`.
    pub(crate) const fn first_after(self, now: Tick) -> Tick {
        if self.next.get() > now.get() {
            return self.next;
        }
        let every = self.every.get();
        let passed = (now.get() - self.next.get()) / if every == 0 { 1 } else { every } + 1;
        Tick::new(self.next.get() + passed * every)
    }
}
