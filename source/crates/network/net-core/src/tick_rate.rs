//! The rate of the fixed schedule, and the conversions between ticks and time it gives: integer
//! arithmetic on the rate and the fixed-point bits keeps every tick count exact, where a
//! multiplication through `f32` loses the sub-tick part past 2^24 ticks.

use core::num::{NonZeroU16, TryFromIntError};
use core::time::Duration;
use fixed::types::U32F32;

const NANOS_PER_SEC: u128 = 1_000_000_000;

/// How many ticks the fixed schedule runs each second (13-net-stack/time.md, The tick length is a
/// rate in hertz, and the timestep is its period).
///
/// A tick lasts `1/hz` s, which no `Duration` holds exactly at most rates: at 60 Hz it is
/// 16,666,666.6… ns. The conversions compute with the rate and round once, so 3 ticks at 60 Hz
/// are exactly 50 ms; only [`Self::period`], the timestep `Time<Fixed>` runs, rounds the tick
/// itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "ecs", derive(bevy_ecs::resource::Resource))]
#[cfg_attr(feature = "reflect", derive(bevy_reflect::Reflect))]
pub struct TickRate(NonZeroU16);

impl TickRate {
    pub const fn new(hz: NonZeroU16) -> Self {
        Self(hz)
    }

    /// The ticks each second.
    pub const fn hz(self) -> NonZeroU16 {
        self.0
    }

    /// One tick's length, rounded to the nearest nanosecond: the timestep of `Time<Fixed>`.
    pub const fn period(self) -> Duration {
        let hz = self.0.get() as u64;
        Duration::from_nanos((1_000_000_000 + hz / 2) / hz)
    }

    /// The time `ticks` take, `ticks · 10⁹ / hz` ns rounded to the nearest nanosecond, and
    /// saturated at [`Duration::MAX`].
    pub fn duration_of(self, ticks: U32F32) -> Duration {
        let denominator = u128::from(self.0.get()) << U32F32::FRAC_NBITS;
        // `ticks` is under 2^64 bits and 10⁹ under 2^30, so the product stays below 2^94.
        let nanos = (u128::from(ticks.to_bits()) * NANOS_PER_SEC + denominator / 2) / denominator;
        let Ok(secs) = u64::try_from(nanos / NANOS_PER_SEC) else {
            return Duration::MAX;
        };
        Duration::new(secs, u32::try_from(nanos % NANOS_PER_SEC).unwrap())
    }

    /// The whole ticks that cover `duration`, `⌈duration · hz⌉`: exact, where a rounded
    /// [`Self::ticks_in`] can lose a nanosecond past a tick.
    pub fn ticks_covering(self, duration: Duration) -> u128 {
        (duration.as_nanos() * u128::from(self.0.get())).div_ceil(NANOS_PER_SEC)
    }

    /// The ticks `duration` spans, `duration · hz` rounded to the nearest 2^-32 tick, and
    /// saturated at [`U32F32::MAX`].
    pub fn ticks_in(self, duration: Duration) -> U32F32 {
        // `Duration::MAX` is under 2^94 ns and `hz` under 2^16: the whole ticks and their
        // remainder are taken apart, so the shift of the fraction stays below 2^62.
        let product = duration.as_nanos() * u128::from(self.0.get());
        let whole = product / NANOS_PER_SEC;
        let rest = product % NANOS_PER_SEC;
        let fraction = ((rest << U32F32::FRAC_NBITS) + NANOS_PER_SEC / 2) / NANOS_PER_SEC;
        let bits = u64::try_from(whole)
            .ok()
            .and_then(|whole| whole.checked_mul(1 << U32F32::FRAC_NBITS))
            .and_then(|whole| whole.checked_add(u64::try_from(fraction).unwrap()));
        U32F32::from_bits(bits.unwrap_or(u64::MAX))
    }
}

impl From<NonZeroU16> for TickRate {
    fn from(hz: NonZeroU16) -> Self {
        Self(hz)
    }
}

/// A rate of `hz` ticks each second, which no rate of zero is.
impl TryFrom<u16> for TickRate {
    type Error = TryFromIntError;

    fn try_from(hz: u16) -> Result<Self, Self::Error> {
        NonZeroU16::try_from(hz).map(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rate(hz: u16) -> TickRate {
        TickRate::try_from(hz).unwrap()
    }

    /// At 60 Hz the period rounds 16,666,666.6… ns to 16,666,667 ns, but 3 ticks are exactly
    /// 50 ms, where 3 rounded periods are 50,000,001 ns, and 1 s is exactly 60 ticks.
    #[test]
    fn a_rate_rounds_once() {
        let sixty = rate(60);
        assert_eq!(sixty.period(), Duration::from_nanos(16_666_667));
        assert_eq!(
            sixty.duration_of(U32F32::from_num(3)),
            Duration::from_millis(50)
        );
        assert_eq!(sixty.ticks_in(Duration::from_secs(1)), U32F32::from_num(60));
    }

    /// 3.5 ticks at 3 Hz are 1.1666… s, which rounds to 1,166,666,667 ns; 1,906,250 µs at 8 Hz
    /// are 15.25 ticks.
    #[test]
    fn conversions_round_to_the_nearest_unit() {
        assert_eq!(
            rate(3).duration_of(U32F32::lit("3.5")),
            Duration::from_nanos(1_166_666_667)
        );
        assert_eq!(
            rate(8).ticks_in(Duration::from_micros(1_906_250)),
            U32F32::lit("15.25")
        );
    }

    /// At 60 Hz, 50 ms are 3 whole ticks, and 1 ns more takes a 4th.
    #[test]
    fn whole_ticks_cover_a_duration_exactly() {
        let sixty = rate(60);
        assert_eq!(sixty.ticks_covering(Duration::from_millis(50)), 3);
        assert_eq!(
            sixty.ticks_covering(Duration::from_millis(50) + Duration::from_nanos(1)),
            4
        );
        assert_eq!(sixty.ticks_covering(Duration::ZERO), 0);
    }

    /// 2^24 + 0.5 ticks at 1000 Hz: `f32` holds 2^24 + 0.5 as 2^24, losing the half tick.
    #[test]
    fn a_conversion_stays_exact_past_the_precision_of_f32() {
        let thousand = rate(1000);
        let ticks = U32F32::from_num(16_777_216) + U32F32::lit("0.5");
        let duration = Duration::from_micros(16_777_216_500);
        assert_eq!(thousand.duration_of(ticks), duration);
        assert_eq!(thousand.ticks_in(duration), ticks);
    }

    /// The largest instant, 2^32 - 2^-32 ticks at 1 Hz, is 2^32 s less 0.23 ns, which rounds to
    /// 2^32 s. A duration past the largest instant saturates.
    #[test]
    fn a_conversion_saturates() {
        assert_eq!(
            rate(1).duration_of(U32F32::MAX),
            Duration::from_secs(1 << 32)
        );
        assert_eq!(rate(1).ticks_in(Duration::MAX), U32F32::MAX);
        assert_eq!(
            rate(u16::MAX).ticks_in(Duration::from_secs(1 << 20)),
            U32F32::MAX
        );
    }
}
