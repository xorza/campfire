use std::num::{NonZeroU32, TryFromIntError};
use std::time::Duration;

use crate::num::Num;
use crate::rounding::Rounding;

const NANOS_PER_SEC: u128 = 1_000_000_000;

/// A rate of ticks: `ticks` every `seconds` whole seconds, so a paced rate, half of 21 Hz among
/// them, 21 ticks every 2 s, stays exact (12-lightyear-in-tree.md, One tick rate). It is kept in
/// lowest terms, so two rates of one value are equal. Its conversions compute with the rate and a
/// `Num`'s bits and round once, where a multiplication through `f32` loses the sub-tick part past
/// 2^24 ticks (13-net-stack/time.md, The tick length is a rate, and the timestep is its period).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TickRate {
    ticks: NonZeroU32,
    seconds: NonZeroU32,
}

impl TickRate {
    /// `hz` ticks every second.
    pub const fn hertz(hz: NonZeroU32) -> Self {
        Self {
            ticks: hz,
            seconds: NonZeroU32::MIN,
        }
    }

    /// `ticks` every `seconds` seconds, in lowest terms.
    pub const fn new(ticks: NonZeroU32, seconds: NonZeroU32) -> Self {
        let divisor = gcd(ticks.get(), seconds.get());
        Self {
            ticks: NonZeroU32::new(ticks.get() / divisor).expect("a divisor of a nonzero count"),
            seconds: NonZeroU32::new(seconds.get() / divisor)
                .expect("a divisor of a nonzero count"),
        }
    }

    /// The ticks of each [`Self::seconds`], in lowest terms.
    pub const fn ticks(self) -> NonZeroU32 {
        self.ticks
    }

    /// The seconds in which [`Self::ticks`] ticks run, in lowest terms.
    pub const fn seconds(self) -> NonZeroU32 {
        self.seconds
    }

    /// The rate in whole hertz; `None` for a rate of a fraction of a hertz, as half of 21 Hz is.
    pub const fn hz(self) -> Option<NonZeroU32> {
        if self.seconds.get() == 1 {
            Some(self.ticks)
        } else {
            None
        }
    }

    /// One tick's length, `seconds · 10⁹ / ticks` ns rounded to the nearest nanosecond, a tie to
    /// the even one, as [`Self::duration_of`] rounds one tick; its own integer step, so a constant
    /// can hold it.
    pub const fn period(self) -> Duration {
        let ticks = self.ticks.get() as u64;
        let nanos = self.seconds.get() as u64 * NANOS_PER_SEC as u64;
        let (whole, rest) = (nanos / ticks, nanos % ticks);
        let up = 2 * rest > ticks || (2 * rest == ticks && whole % 2 == 1);
        Duration::from_nanos(if up { whole + 1 } else { whole })
    }

    /// The time `ticks` take, `ticks · seconds · 10⁹ / rate ticks` ns rounded to the nearest
    /// nanosecond, and saturated at [`Duration::MAX`]. `ticks` is not negative.
    pub fn duration_of(self, ticks: Num) -> Duration {
        debug_assert!(ticks >= Num::ZERO, "a duration of {ticks} ticks");
        // `ticks` is under 2^63 bits, `seconds` under 2^32 and 10⁹ under 2^30, so the product
        // stays below 2^125.
        let numerator = i128::from(ticks.to_bits())
            * i128::from(self.seconds.get())
            * i128::try_from(NANOS_PER_SEC).unwrap();
        let denominator = i128::from(self.ticks.get()) << Num::FRAC_BITS;
        let nanos = u128::try_from(Rounding::NearestEven.divide(numerator, denominator)).unwrap();
        let Ok(secs) = u64::try_from(nanos / NANOS_PER_SEC) else {
            return Duration::MAX;
        };
        Duration::new(secs, u32::try_from(nanos % NANOS_PER_SEC).unwrap())
    }

    /// The ticks `duration` spans, rounded to the nearest 2^-24 tick, and saturated at
    /// [`Num::MAX`].
    pub fn ticks_in(self, duration: Duration) -> Num {
        // `Duration::MAX` is under 2^94 ns and the rate's ticks under 2^32: the whole ticks and
        // their remainder are taken apart, so the shift of the fraction stays below 2^86.
        let product = duration.as_nanos() * u128::from(self.ticks.get());
        let denominator = u128::from(self.seconds.get()) * NANOS_PER_SEC;
        let whole = product / denominator;
        let fraction = Rounding::NearestEven.divide(
            i128::try_from((product % denominator) << Num::FRAC_BITS).unwrap(),
            i128::try_from(denominator).unwrap(),
        );
        match i128::try_from(whole) {
            Ok(whole) if whole < 1 << 63 => {
                Num::saturating_from_bits((whole << Num::FRAC_BITS) + fraction)
            }
            _ => Num::MAX,
        }
    }

    /// The whole ticks that cover `duration`: exact, where a rounded [`Self::ticks_in`] can lose a
    /// nanosecond past a tick.
    pub fn ticks_covering(self, duration: Duration) -> u128 {
        (duration.as_nanos() * u128::from(self.ticks.get()))
            .div_ceil(u128::from(self.seconds.get()) * NANOS_PER_SEC)
    }
}

/// A rate of `hz` ticks every second, which no rate of zero is.
impl TryFrom<u32> for TickRate {
    type Error = TryFromIntError;

    fn try_from(hz: u32) -> Result<Self, Self::Error> {
        NonZeroU32::try_from(hz).map(Self::hertz)
    }
}

const fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

#[cfg(test)]
mod tests;
