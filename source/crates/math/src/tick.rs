use std::fmt;

use serde::{Deserialize, Serialize};

/// A time in the match, in ticks from its start: tick `t` starts at time `t` and ends at `t + 1`.
#[must_use]
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
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

/// As its number.
impl fmt::Display for Tick {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Ticks {
    pub const ZERO: Ticks = Ticks(0);
    pub const ONE: Ticks = Ticks(1);

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
    }
}
