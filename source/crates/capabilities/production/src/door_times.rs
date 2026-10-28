use campfire_common::Ticks;
use campfire_sim::MatchRate;

use crate::doors_data::DoorsData;

/// How long each of a producer's doors opens, stays open after its last unit, and closes, in ticks,
/// and how many it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DoorTimes {
    pub count: u8,
    pub opening: Ticks,
    pub open: Ticks,
    pub closing: Ticks,
}

impl DoorTimes {
    /// The times of `data` at `rate`, each rounded up, so no door opens early.
    pub(crate) fn of(data: DoorsData, rate: MatchRate) -> Self {
        let ticks = |ms: u32| {
            rate.ticks(u64::from(ms))
                .expect("a u32 of milliseconds at a u32 rate counts in a u64 of ticks")
        };
        Self {
            count: data.count.get(),
            opening: ticks(data.opening_ms),
            open: ticks(data.open_ms),
            closing: ticks(data.closing_ms),
        }
    }
}
