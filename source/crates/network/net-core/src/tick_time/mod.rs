use crate::net_tick::NetTick;
use crate::overstep::Overstep;
use crate::tick_range_error::TickRangeError;
#[cfg(feature = "reflect")]
use bevy_reflect::Reflect;
use campfire_math::{Num, Rounding, TickRate};
use campfire_net_bytes::ReadInteger;
use campfire_net_bytes::Reader;
use campfire_net_bytes::SerializationError;
use campfire_net_bytes::ToBytes;
use campfire_net_bytes::WriteInteger;
use core::fmt::Debug;
use core::fmt::Formatter;
use core::num::{NonZeroI128, NonZeroU32};
use core::ops::Add;
use core::ops::Neg;
use core::ops::Sub;
use core::time::Duration;

/// The bits of the largest unsigned span and instant, 2^32 - 2^-24 ticks.
const UNSIGNED_MAX: i64 = (1 << (32 + Num::FRAC_BITS)) - 1;
/// The bits of the largest signed span, 2^31 - 2^-24 ticks.
const SIGNED_MAX: i64 = (1 << (31 + Num::FRAC_BITS)) - 1;
/// The bits of the smallest signed span, -2^31 ticks.
const SIGNED_MIN: i64 = -(1 << (31 + Num::FRAC_BITS));

/// The number of `bits` within `[min, max]`, at the nearer end when past it.
const fn clamp(bits: i128, min: i64, max: i64) -> Num {
    if bits < min as i128 {
        Num::from_bits(min)
    } else if bits > max as i128 {
        Num::from_bits(max)
    } else {
        Num::from_bits(bits as i64)
    }
}

/// `value`, when it lies within `[min, max]`.
const fn within(value: Num, min: i64, max: i64) -> Result<Num, TickRangeError> {
    if value.to_bits() >= min && value.to_bits() <= max {
        Ok(value)
    } else {
        Err(TickRangeError)
    }
}

/// A position on a timeline: a whole tick and the fraction of the next one, a `Num` in `[0, 2^32)`
/// ticks (11-net-stack/time.md, Conversions are exact; 11-net-stack/time.md, One number type).
///
/// Arithmetic saturates at that range, as [`NetTick`]'s does at its own.
#[derive(Default, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
#[cfg_attr(feature = "reflect", derive(Reflect))]
#[cfg_attr(feature = "reflect", reflect(opaque))]
pub struct TickInstant {
    value: Num,
}

impl Debug for TickInstant {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl TickInstant {
    pub const ZERO: Self = Self { value: Num::ZERO };

    pub const fn value(self) -> Num {
        self.value
    }

    pub const fn tick(self) -> NetTick {
        NetTick((self.value.to_bits().cast_unsigned() >> Num::FRAC_BITS) as u32)
    }

    /// The fraction of the next tick this instant has run.
    pub const fn overstep(self) -> Overstep {
        Overstep::from_fraction_bits(self.value.to_bits().cast_unsigned() as u32)
    }

    /// The instant `overstep` into the tick after `tick`.
    pub const fn from_tick_and_overstep(tick: NetTick, overstep: Overstep) -> Self {
        Self {
            value: Num::from_bits(((tick.0 as i64) << Num::FRAC_BITS) | overstep.value().to_bits()),
        }
    }

    /// The time since tick 0 at `rate`, rounded to the nearest nanosecond.
    pub fn to_duration(self, rate: TickRate) -> Duration {
        rate.duration_of(self.value)
    }

    /// The instant `duration` after tick 0 at `rate`, rounded to the nearest 2^-24 tick and
    /// saturated at the last one.
    pub fn from_duration(duration: Duration, rate: TickRate) -> Self {
        Self {
            value: clamp(rate.ticks_in(duration).to_bits().into(), 0, UNSIGNED_MAX),
        }
    }
}

impl TryFrom<Num> for TickInstant {
    type Error = TickRangeError;

    fn try_from(value: Num) -> Result<Self, Self::Error> {
        within(value, 0, UNSIGNED_MAX).map(|value| Self { value })
    }
}

impl TryFrom<TickSpan> for TickInstant {
    type Error = TickRangeError;

    fn try_from(value: TickSpan) -> Result<Self, Self::Error> {
        Self::try_from(value.value)
    }
}

impl From<NetTick> for TickInstant {
    fn from(value: NetTick) -> Self {
        Self::from_tick_and_overstep(value, Overstep::default())
    }
}

impl Add<TickSpan> for TickInstant {
    type Output = Self;

    fn add(self, rhs: TickSpan) -> Self::Output {
        let bits = i128::from(self.value.to_bits()) + i128::from(rhs.value.to_bits());
        Self {
            value: clamp(bits, 0, UNSIGNED_MAX),
        }
    }
}

impl Sub<TickSpan> for TickInstant {
    type Output = Self;

    fn sub(self, rhs: TickSpan) -> Self::Output {
        let bits = i128::from(self.value.to_bits()) - i128::from(rhs.value.to_bits());
        Self {
            value: clamp(bits, 0, UNSIGNED_MAX),
        }
    }
}

impl Sub for TickInstant {
    type Output = TickSpan;

    fn sub(self, rhs: Self) -> Self::Output {
        let bits = i128::from(self.value.to_bits()) - i128::from(rhs.value.to_bits());
        TickSpan {
            value: clamp(bits, SIGNED_MIN, SIGNED_MAX),
        }
    }
}

/// The signed difference between two [`TickInstant`]s, a `Num` in `[-2^31, 2^31)` ticks
/// (11-net-stack/time.md, One number type).
///
/// Arithmetic saturates at that range, as [`NetTick`]'s does at its own.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "reflect", derive(Reflect))]
#[cfg_attr(feature = "reflect", reflect(opaque))]
pub struct TickSpan {
    value: Num,
}

impl Debug for TickSpan {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl TryFrom<Num> for TickSpan {
    type Error = TickRangeError;

    fn try_from(value: Num) -> Result<Self, Self::Error> {
        within(value, SIGNED_MIN, SIGNED_MAX).map(|value| Self { value })
    }
}

impl TryFrom<NetTick> for TickSpan {
    type Error = TickRangeError;

    fn try_from(value: NetTick) -> Result<Self, Self::Error> {
        i32::try_from(value.0)
            .map(Self::from)
            .map_err(|_| TickRangeError)
    }
}

impl From<i32> for TickSpan {
    fn from(value: i32) -> Self {
        Self {
            value: Num::int(i64::from(value)),
        }
    }
}

impl TryFrom<PositiveTickSpan> for TickSpan {
    type Error = TickRangeError;

    fn try_from(value: PositiveTickSpan) -> Result<Self, Self::Error> {
        Self::try_from(value.value)
    }
}

impl TryFrom<TickInstant> for TickSpan {
    type Error = TickRangeError;

    fn try_from(value: TickInstant) -> Result<Self, Self::Error> {
        Self::try_from(value.value)
    }
}

impl TickSpan {
    pub const ZERO: Self = Self { value: Num::ZERO };
    pub const MAX: Self = Self {
        value: Num::from_bits(SIGNED_MAX),
    };
    pub const MIN: Self = Self {
        value: Num::from_bits(SIGNED_MIN),
    };

    pub const fn value(self) -> Num {
        self.value
    }

    pub const fn is_positive(self) -> bool {
        self.value.to_bits() > 0
    }

    pub const fn is_negative(self) -> bool {
        self.value.to_bits() < 0
    }

    /// The ticks `duration` spans at `rate`, rounded to the nearest 2^-24 tick and saturated at
    /// the largest span.
    pub fn from_duration(duration: Duration, rate: TickRate) -> Self {
        Self {
            value: clamp(rate.ticks_in(duration).to_bits().into(), 0, SIGNED_MAX),
        }
    }

    /// The span times `numerator / denominator`, rounded once to the nearest 2^-24 tick, a tie
    /// to the even one, and saturated at the span's range.
    #[must_use]
    pub fn mul_ratio(self, numerator: u32, denominator: NonZeroU32) -> Self {
        let bits = Rounding::NearestEven.divide(
            i128::from(self.value.to_bits()) * i128::from(numerator),
            NonZeroI128::from(denominator).get(),
        );
        Self {
            value: clamp(bits, SIGNED_MIN, SIGNED_MAX),
        }
    }

    /// The size of the span: exact, as the smallest span is `-2^31` and the largest unsigned
    /// span is past `2^31`.
    pub const fn unsigned_abs(self) -> PositiveTickSpan {
        PositiveTickSpan {
            value: Num::from_bits(self.value.to_bits().abs()),
        }
    }

    /// The span in whole ticks, rounded to the nearest, a half away from zero, and saturated at
    /// `i32`'s range, which the largest span rounds past.
    pub fn to_i32(self) -> i32 {
        let ticks = self.value.round();
        i32::try_from(ticks).unwrap_or(if ticks < 0 { i32::MIN } else { i32::MAX })
    }
}

impl Neg for TickSpan {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self {
            value: clamp(-i128::from(self.value.to_bits()), SIGNED_MIN, SIGNED_MAX),
        }
    }
}

impl Add for TickSpan {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        let bits = i128::from(self.value.to_bits()) + i128::from(rhs.value.to_bits());
        Self {
            value: clamp(bits, SIGNED_MIN, SIGNED_MAX),
        }
    }
}

impl Sub for TickSpan {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        let bits = i128::from(self.value.to_bits()) - i128::from(rhs.value.to_bits());
        Self {
            value: clamp(bits, SIGNED_MIN, SIGNED_MAX),
        }
    }
}

/// A non-negative [`TickSpan`], as an interval travels on the wire, a `Num` in `[0, 2^32)` ticks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
#[cfg_attr(feature = "reflect", derive(Reflect))]
#[cfg_attr(feature = "reflect", reflect(opaque))]
pub struct PositiveTickSpan {
    value: Num,
}

impl PositiveTickSpan {
    /// `ticks` whole ticks.
    pub const fn from_ticks(ticks: u32) -> Self {
        Self {
            value: Num::from_bits((ticks as i64) << Num::FRAC_BITS),
        }
    }

    pub const fn value(self) -> Num {
        self.value
    }

    /// The whole ticks of the span, rounded down.
    pub const fn whole_ticks(self) -> u32 {
        (self.value.to_bits().cast_unsigned() >> Num::FRAC_BITS) as u32
    }

    pub const fn overstep(self) -> Overstep {
        Overstep::from_fraction_bits(self.value.to_bits().cast_unsigned() as u32)
    }

    /// The time these ticks take at `rate`, rounded to the nearest nanosecond.
    pub fn to_duration(self, rate: TickRate) -> Duration {
        rate.duration_of(self.value)
    }

    /// The ticks `duration` spans at `rate`, rounded to the nearest 2^-24 tick and saturated at
    /// the largest span.
    pub fn from_duration(duration: Duration, rate: TickRate) -> Self {
        Self {
            value: clamp(rate.ticks_in(duration).to_bits().into(), 0, UNSIGNED_MAX),
        }
    }
}

impl TryFrom<Num> for PositiveTickSpan {
    type Error = TickRangeError;

    fn try_from(value: Num) -> Result<Self, Self::Error> {
        within(value, 0, UNSIGNED_MAX).map(|value| Self { value })
    }
}

impl TryFrom<TickSpan> for PositiveTickSpan {
    type Error = TickRangeError;

    fn try_from(value: TickSpan) -> Result<Self, Self::Error> {
        Self::try_from(value.value)
    }
}

/// Its whole ticks in 4 bytes and its overstep's top 8 bits in one.
impl ToBytes for PositiveTickSpan {
    fn bytes_len(&self) -> usize {
        4 + self.overstep().bytes_len()
    }

    fn to_bytes(&self, buffer: &mut impl WriteInteger) {
        buffer.write_u32(self.whole_ticks());
        self.overstep().to_bytes(buffer);
    }

    fn from_bytes(buffer: &mut Reader) -> Result<Self, SerializationError>
    where
        Self: Sized,
    {
        let whole_ticks = buffer.read_u32()?;
        let overstep = Overstep::from_bytes(buffer)?;
        Ok(Self {
            value: Num::from_bits(
                (i64::from(whole_ticks) << Num::FRAC_BITS) | overstep.value().to_bits(),
            ),
        })
    }
}

#[cfg(test)]
mod tests;
