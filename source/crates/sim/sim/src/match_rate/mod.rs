use std::num::NonZeroU32;
use std::time::Duration;

use bevy_ecs::resource::Resource;
use campfire_common::Ticks;
use campfire_math::TickRate;

/// The match's tick rate, fixed for the whole match by the session's terms, in whole hertz as the
/// terms give it. Data gives times in milliseconds and rates per second; they become ticks with
/// it. A paced client's timelines run at a rate of their own, so the sim's rate never changes with
/// the speed (14-workspace-alignment.md, One tick rate type, two resources).
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchRate(TickRate);

impl MatchRate {
    pub const fn new(hz: NonZeroU32) -> Self {
        Self(TickRate::hertz(hz))
    }

    pub const fn hz(self) -> NonZeroU32 {
        self.0.ticks()
    }

    /// How long a tick lasts, to the nearest nanosecond.
    pub const fn length(self) -> Duration {
        self.0.period()
    }

    /// `ms` milliseconds in ticks, rounded up, so nothing happens early; `None` when `ms` times
    /// the rate does not fit a `u64`.
    pub const fn ticks(self, ms: u64) -> Option<Ticks> {
        match ms.checked_mul(self.hz().get() as u64) {
            Some(scaled) => Some(Ticks::new(scaled.div_ceil(1000))),
            None => None,
        }
    }

    /// `ms` as how long something lasts: in ticks, rounded up, a tick at the least, so what
    /// starts also takes effect; `None` when it does not fit a `u64`.
    pub const fn duration(self, ms: u64) -> Option<Ticks> {
        match self.ticks(ms) {
            Some(ticks) if ticks.get() == 0 => Some(Ticks::ONE),
            ticks => ticks,
        }
    }

    /// `ms` as a window back from now: in ticks, rounded up; every tick there is when `ms` times
    /// the rate does not fit a `u64`.
    pub const fn window(self, ms: u64) -> Ticks {
        match self.ticks(ms) {
            Some(ticks) => ticks,
            None => Ticks::new(u64::MAX),
        }
    }

    /// `ms` as a cut of a wait: in ticks, rounded down, so a cut ends no wait before its time;
    /// every tick there is when `ms` times the rate does not fit a `u64`.
    pub const fn cut(self, ms: u64) -> Ticks {
        match ms.checked_mul(self.hz().get() as u64) {
            Some(scaled) => Ticks::new(scaled / 1000),
            None => Ticks::new(u64::MAX),
        }
    }
}

#[cfg(test)]
mod tests;
