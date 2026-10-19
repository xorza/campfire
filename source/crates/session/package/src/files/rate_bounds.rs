use std::num::NonZeroU32;

use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::files::tick_range::TickRange;

/// The tick rates a rules package runs at, in ticks a second: from `min` to `max`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateBounds {
    min: NonZeroU32,
    max: NonZeroU32,
}

impl RateBounds {
    /// The rates from `min` to `max`; `None` when `min` is past `max`.
    pub const fn new(min: NonZeroU32, max: NonZeroU32) -> Option<Self> {
        if min.get() > max.get() {
            return None;
        }
        Some(Self { min, max })
    }

    /// Whether every rate of `range` is one of its own.
    pub const fn holds(self, range: TickRange) -> bool {
        self.min.get() <= range.slowest().get() && range.fastest().get() <= self.max.get()
    }
}

impl<'de> Deserialize<'de> for RateBounds {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            min: NonZeroU32,
            max: NonZeroU32,
        }
        let Fields { min, max } = Fields::deserialize(deserializer)?;
        Self::new(min, max).ok_or_else(|| D::Error::custom("the tick rates' min is past their max"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_hold_a_range_that_lies_within_them() {
        let hz = |value| NonZeroU32::new(value).unwrap();
        let bounds = RateBounds::new(hz(20), hz(30)).unwrap();
        let range = |min, default, max| TickRange::new(hz(min), hz(default), hz(max)).unwrap();
        // From 20 to 30 holds 20 to 30 and 30 alone, not 10 to 30 nor 20 to 60.
        assert!(bounds.holds(range(20, 30, 30)));
        assert!(bounds.holds(range(30, 30, 30)));
        assert!(!bounds.holds(range(10, 20, 30)));
        assert!(!bounds.holds(range(20, 30, 60)));
        assert_eq!(RateBounds::new(hz(31), hz(30)), None);
    }
}
