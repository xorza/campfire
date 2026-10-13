use std::num::NonZeroU32;

use serde::de::Error;
use serde::{Deserialize, Deserializer};

/// The tick rates a session may choose, in ticks a second: `min ≤ default ≤ max`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TickRange {
    min: NonZeroU32,
    max: NonZeroU32,
    default: NonZeroU32,
}

impl TickRange {
    /// The range from `min` to `max` with its `default`; `None` unless it holds its default.
    pub const fn new(min: NonZeroU32, default: NonZeroU32, max: NonZeroU32) -> Option<TickRange> {
        if min.get() > default.get() || default.get() > max.get() {
            return None;
        }
        Some(TickRange { min, max, default })
    }

    pub const fn contains(self, hz: NonZeroU32) -> bool {
        self.min.get() <= hz.get() && hz.get() <= self.max.get()
    }

    pub const fn default(self) -> NonZeroU32 {
        self.default
    }

    /// The fastest rate, at which a time counts the most ticks: a time that counts in ticks at
    /// it counts at every rate of the range.
    pub const fn fastest(self) -> NonZeroU32 {
        self.max
    }
}

impl<'de> Deserialize<'de> for TickRange {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<TickRange, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            min: NonZeroU32,
            max: NonZeroU32,
            default: NonZeroU32,
        }
        let Fields { min, max, default } = Fields::deserialize(deserializer)?;
        TickRange::new(min, default, max)
            .ok_or_else(|| D::Error::custom("the tick rate range does not hold its default"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tick_range_holds_its_default() {
        let hz = |value| NonZeroU32::new(value).unwrap();
        let range = TickRange::new(hz(20), hz(30), hz(60)).unwrap();
        assert_eq!(range.default(), hz(30));
        let held = [19, 20, 60, 61].map(|value| range.contains(hz(value)));
        assert_eq!(held, [false, true, true, false]);
        assert!(TickRange::new(hz(30), hz(30), hz(30)).is_some());
        assert!(TickRange::new(hz(40), hz(30), hz(60)).is_none());
        assert!(TickRange::new(hz(20), hz(61), hz(60)).is_none());
    }
}
