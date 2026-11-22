use campfire_actions::ActionKind;
use campfire_common::Ticks;
use campfire_math::{Num, Rounding};
use campfire_values::Share;

use crate::error::ProductionDataError;
use crate::mode_power_data::{EmptyPower, PowerSpeedData};
use crate::player_power::PlayerPower;

/// A short player's speed of production, checked: `min` above 0 and at most `max`, and the kinds
/// it slows production's timed ones (power.md, The speed by the ratio).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PowerSpeed {
    penalty: Share,
    min: Share,
    max: Share,
    empty: EmptyPower,
    slows: SlowedKinds,
}

/// The kinds of action a speed slows, a bit each by the kind's place.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct SlowedKinds(u8);

/// A speed as a fraction, `over` above 0.
#[derive(Debug, Clone, Copy)]
struct Fraction {
    of: u128,
    over: u128,
}

impl PowerSpeed {
    /// The speed `data` gives; an error for a `min` of 0 or past `max`, or a kind production
    /// does not time.
    pub(crate) fn new(data: &PowerSpeedData) -> Result<Self, ProductionDataError> {
        if data.min.is_zero() {
            return Err(ProductionDataError::PowerMin);
        }
        if data.min > data.max {
            return Err(ProductionDataError::PowerMinPastMax);
        }
        let mut slows = SlowedKinds::default();
        for &kind in &data.slows {
            if !matches!(
                kind,
                ActionKind::Train | ActionKind::Research | ActionKind::Build
            ) {
                return Err(ProductionDataError::PowerSlows(kind));
            }
            slows.0 |= 1 << kind as u8;
        }
        Ok(Self {
            penalty: data.penalty,
            min: data.min,
            max: data.max,
            empty: data.empty,
            slows,
        })
    }

    /// Whether it changes the time of an action of `kind`.
    pub(crate) const fn slows(self, kind: ActionKind) -> bool {
        self.slows.0 & (1 << kind as u8) != 0
    }

    /// `time` at the speed of a player of `power`: the time over the speed, rounded down, from
    /// the exact counts, as `ThingTemplate::calcTimeToBuild` divides by its rate and truncates;
    /// `time` itself for a player that is not short and gives some of its use, or for one that
    /// gives and takes none at an `empty` of `full`. A time past what ticks count is the most
    /// they count, which no match reaches.
    pub(crate) fn time_at(self, time: Ticks, power: PlayerPower) -> Ticks {
        let Some(speed) = self.speed(power) else {
            return time;
        };
        let slowed = Self::mul_div(time.get(), speed.over, speed.of);
        Ticks::new(u64::try_from(slowed).unwrap_or(u64::MAX))
    }

    /// What a site of `time` ticks, a whole count, grows a tick at its builders' `rate` for a
    /// player of `power`: the rate times its time over its time at that power, rounded up, so at
    /// a constant power it completes in that time, never later (power.md, A site grows at the
    /// speed).
    pub(crate) fn site_rate(self, rate: Num, time: Num, power: PlayerPower) -> Num {
        let whole = time.to_int().expect("a site's time is whole ticks");
        let ticks = Ticks::new(u64::try_from(whole).expect("a site's time is at least 0"));
        let slowed = self.time_at(ticks, power).get();
        if slowed == ticks.get() {
            return rate;
        }
        let slowed = i64::try_from(slowed).unwrap_or(i64::MAX);
        rate.checked_mul_div_int(whole, slowed, Rounding::Ceiling)
            .expect("a rate slowed is at most the rate")
    }

    /// `time` at the slowest this speed goes, its `min`, rounded down.
    pub(crate) fn slowest(self, time: Ticks) -> Ticks {
        let whole = u128::from(Share::ALL.parts());
        let slowed = Self::mul_div(time.get(), whole, u128::from(self.min.parts()));
        Ticks::new(u64::try_from(slowed).unwrap_or(u64::MAX))
    }

    /// The speed of a player of `power`, none for full speed: `1 − (1 − share) × penalty`, the
    /// share its given over its used power, at least `min`, and at most `max`.
    fn speed(self, power: PlayerPower) -> Option<Fraction> {
        let given = Self::bits(power.given());
        let used = Self::bits(power.used());
        let full = if used == 0 {
            given > 0 || self.empty == EmptyPower::Full
        } else {
            given >= used
        };
        if full {
            return None;
        }
        // A player that gives and takes none has a share of 0, as 0 of 1.
        let used = used.max(1);
        let whole = u128::from(Share::ALL.parts());
        // Of at most 2⁶³ bits, each product here fits 2¹²³.
        let over = used * whole;
        let short = (used - given) * u128::from(self.penalty.parts());
        let mut speed = Fraction {
            of: over - short,
            over,
        };
        let min = u128::from(self.min.parts());
        if speed.of * whole < min * speed.over {
            speed = Fraction {
                of: min,
                over: whole,
            };
        }
        let max = u128::from(self.max.parts());
        if speed.of * whole > max * speed.over {
            speed = Fraction {
                of: max,
                over: whole,
            };
        }
        debug_assert!(speed.of > 0, "a speed of a `min` above 0 is above 0");
        debug_assert!(
            speed.of <= speed.over,
            "a short player's speed is at most 1"
        );
        Some(speed)
    }

    /// A power's bits, 0 for none: a count is at least 0.
    fn bits(power: Num) -> u128 {
        u128::try_from(power.to_bits()).expect("a power count is at least 0")
    }

    /// `⌊a × b ÷ c⌋`, exactly, for `b ÷ c` below 2⁶² and `c` below 2¹²⁴: `a × b` may pass 2¹²⁸, so
    /// the rest of `b ÷ c` times `a` goes in two halves of 32 bits.
    fn mul_div(a: u64, b: u128, c: u128) -> u128 {
        debug_assert!(c > 0);
        debug_assert!(c < 1 << 124);
        let (whole, rest) = (b / c, b % c);
        debug_assert!(
            whole < 1 << 62,
            "a speed of a `min` above 0 slows less than 2⁶² times"
        );
        let (high, low) = (u128::from(a >> 32), u128::from(a & u64::from(u32::MAX)));
        // a × rest ÷ c = high × (rest × 2³²) ÷ c + low × rest ÷ c, each product below 2¹²⁶.
        let shifted = rest << 32;
        let (high_whole, high_rest) = (shifted / c, shifted % c);
        u128::from(a) * whole + high * high_whole + (high * high_rest + low * rest) / c
    }
}

#[cfg(test)]
mod tests;
