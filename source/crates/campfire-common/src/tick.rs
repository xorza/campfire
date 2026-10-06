use derive_more::Display;
use serde::{Deserialize, Serialize};

/// A time in the match, in ticks from its start: tick `t` starts at time `t` and ends at `t + 1`.
#[must_use]
#[derive(
    Debug,
    Display,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
)]
#[serde(transparent)]
pub struct Tick(u64);

/// A length of time in ticks.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Ticks(u64);

impl Tick {
    pub const ZERO: Tick = Tick(0);
    /// The latest time a match makes, and the most of every count its state holds: 2⁶², which no
    /// match reaches, so no sum of two restored values overflows.
    pub const LIMIT: Tick = Tick(1 << 62);

    pub const fn new(tick: u64) -> Tick {
        Tick(tick)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    /// The time `ticks` after this one. A match never runs long enough to pass `u64::MAX`.
    pub const fn after(self, ticks: Ticks) -> Tick {
        Tick(self.0.checked_add(ticks.0).expect("tick numbers exhausted"))
    }

    /// The ticks from `earlier` to this time; `None` when `earlier` is later.
    pub const fn since(self, earlier: Tick) -> Option<Ticks> {
        match self.0.checked_sub(earlier.0) {
            Some(ticks) => Some(Ticks(ticks)),
            None => None,
        }
    }
}

impl Ticks {
    pub const ZERO: Ticks = Ticks(0);
    pub const ONE: Ticks = Ticks(1);
    /// The longest time a match makes: `Tick::LIMIT`'s.
    pub const LIMIT: Ticks = Ticks(Tick::LIMIT.0);

    pub const fn new(ticks: u64) -> Ticks {
        Ticks(ticks)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_time_after_a_length_is_the_sum_and_since_gives_it_back() {
        let (start, length) = (Tick::new(7), Ticks::new(5));
        assert_eq!(start.after(length), Tick::new(12));
        assert_eq!(Tick::new(12).since(start), Some(length));
        assert_eq!(start.since(Tick::new(12)), None);
        assert_eq!(start.after(Ticks::ZERO), start);
        let encoded = postcard::to_allocvec(&(Tick::new(300), Ticks::new(300))).unwrap();
        assert_eq!(encoded, postcard::to_allocvec(&(300_u64, 300_u32)).unwrap());
        assert_eq!(Tick::new(300).to_string(), "300");
        // Two values at the limit sum to 2⁶³, within a `u64`.
        assert_eq!(Tick::LIMIT.after(Ticks::LIMIT), Tick::new(1 << 63));
    }
}
