use crate::net_tick::NetTick;
use crate::overstep::Overstep;
use crate::tick_range_error::TickRangeError;
use crate::tick_rate::TickRate;
#[cfg(feature = "reflect")]
use bevy_reflect::Reflect;
use campfire_net_bytes::ReadInteger;
use campfire_net_bytes::Reader;
use campfire_net_bytes::SerializationError;
use campfire_net_bytes::ToBytes;
use campfire_net_bytes::WriteInteger;
use core::fmt::Debug;
use core::fmt::Formatter;
use core::num::NonZeroU32;
use core::ops::Add;
use core::ops::Neg;
use core::ops::Sub;
use core::time::Duration;
use fixed::traits::ToFixed;
use fixed::types::I32F32;
use fixed::types::U32F32;

/// A position on a timeline: a whole tick and the fraction of the next one.
///
/// Arithmetic saturates at the `U32F32` range, as [`NetTick`]'s does at its own.
#[derive(Default, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
#[cfg_attr(feature = "reflect", derive(Reflect))]
#[cfg_attr(feature = "reflect", reflect(opaque))]
pub struct TickInstant {
    pub value: U32F32,
}

impl Debug for TickInstant {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl TickInstant {
    pub const fn lit(src: &str) -> Self {
        Self {
            value: U32F32::lit(src),
        }
    }
    pub const fn zero() -> Self {
        Self {
            value: U32F32::ZERO,
        }
    }
    pub fn tick(&self) -> NetTick {
        NetTick(self.value.to_num())
    }
    /// Overstep as a fraction towards the next tick
    pub fn overstep(&self) -> Overstep {
        Overstep::new(self.value.wrapping_to_fixed())
    }

    /// Construct a [`TickInstant`] from an integer tick and a fractional overstep.
    ///
    /// `tick` is the whole tick count, and `overstep` is the fraction towards
    /// the next tick in the range [0, 1).
    pub fn from_tick_and_overstep(tick: NetTick, overstep: Overstep) -> Self {
        let base: U32F32 = tick.0.to_fixed();
        let frac: U32F32 = overstep.value().into();
        Self { value: base + frac }
    }

    /// The time since tick 0 at `rate`, rounded to the nearest nanosecond.
    pub fn to_duration(&self, rate: TickRate) -> Duration {
        rate.duration_of(self.value)
    }

    /// The instant `duration` after tick 0 at `rate`, exact to 2^-32 tick and saturated at the
    /// last one.
    pub fn from_duration(duration: Duration, rate: TickRate) -> Self {
        Self {
            value: rate.ticks_in(duration),
        }
    }
}

impl TryFrom<TickDelta> for TickInstant {
    type Error = TickRangeError;

    fn try_from(value: TickDelta) -> Result<Self, Self::Error> {
        value
            .value
            .checked_to_fixed()
            .map(|value| Self { value })
            .ok_or(TickRangeError)
    }
}

impl From<NetTick> for TickInstant {
    fn from(value: NetTick) -> Self {
        Self {
            value: value.0.to_fixed(),
        }
    }
}

impl Add<TickDelta> for TickInstant {
    type Output = Self;

    fn add(self, rhs: TickDelta) -> Self::Output {
        Self {
            value: self.value.saturating_add_signed(rhs.value),
        }
    }
}

impl Sub<TickDelta> for TickInstant {
    type Output = Self;

    fn sub(self, rhs: TickDelta) -> Self::Output {
        Self {
            value: self.value.saturating_sub_signed(rhs.value),
        }
    }
}

impl Sub for TickInstant {
    type Output = TickDelta;

    fn sub(self, rhs: Self) -> Self::Output {
        let bits = i128::from(self.value.to_bits()) - i128::from(rhs.value.to_bits());
        let bits = i64::try_from(bits).unwrap_or(if bits < 0 { i64::MIN } else { i64::MAX });
        TickDelta::new(I32F32::from_bits(bits))
    }
}

/// The signed difference between two [`TickInstant`]s, in fractional ticks.
///
/// Arithmetic saturates at the `I32F32` range, as [`NetTick`]'s does at its own.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "reflect", derive(Reflect))]
#[cfg_attr(feature = "reflect", reflect(opaque))]
pub struct TickDelta {
    /// This is the combined representation of a signed 32-bit tick diff
    /// plus an unsigned 32-bit overstep (range 0 to ~0.99998, always positive).
    pub(crate) value: I32F32,
}

impl Debug for TickDelta {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:.6}", self.value)
    }
}

impl TryFrom<NetTick> for TickDelta {
    type Error = TickRangeError;

    fn try_from(value: NetTick) -> Result<Self, Self::Error> {
        i32::try_from(value.0)
            .map(Self::from)
            .map_err(|_| TickRangeError)
    }
}

impl From<i32> for TickDelta {
    fn from(value: i32) -> Self {
        Self {
            value: value.to_fixed(),
        }
    }
}

impl TryFrom<PositiveTickDelta> for TickDelta {
    type Error = TickRangeError;

    fn try_from(value: PositiveTickDelta) -> Result<Self, Self::Error> {
        value
            .value
            .checked_to_fixed()
            .map(Self::new)
            .ok_or(TickRangeError)
    }
}

impl TryFrom<TickInstant> for TickDelta {
    type Error = TickRangeError;

    fn try_from(value: TickInstant) -> Result<Self, Self::Error> {
        value
            .value
            .checked_to_fixed()
            .map(Self::new)
            .ok_or(TickRangeError)
    }
}

impl TickDelta {
    pub const fn new(value: I32F32) -> Self {
        Self { value }
    }
    pub const fn lit(src: &str) -> Self {
        Self {
            value: I32F32::lit(src),
        }
    }

    pub const fn is_positive(&self) -> bool {
        self.value.is_positive()
    }

    pub const fn is_negative(&self) -> bool {
        self.value.is_negative()
    }

    /// The ticks `duration` spans at `rate`, exact to 2^-32 tick and saturated at the largest
    /// delta.
    pub fn from_duration(duration: Duration, rate: TickRate) -> Self {
        Self::new(rate.ticks_in(duration).saturating_to_fixed())
    }

    /// The delta times `numerator / denominator`, rounded once to the nearest 2^-32 tick, a half
    /// up, and saturated at the `I32F32` range.
    #[must_use]
    pub const fn mul_ratio(self, numerator: u32, denominator: NonZeroU32) -> Self {
        let denominator = denominator.get() as i128;
        let doubled = 2 * self.value.to_bits() as i128 * numerator as i128 + denominator;
        let bits = doubled.div_euclid(2 * denominator);
        let bits = if bits > i64::MAX as i128 {
            i64::MAX
        } else if bits < i64::MIN as i128 {
            i64::MIN
        } else {
            bits as i64
        };
        Self::new(I32F32::from_bits(bits))
    }

    /// The size of the delta: exact, as `I32F32`'s smallest value is `-2^31`.
    pub const fn unsigned_abs(self) -> PositiveTickDelta {
        PositiveTickDelta {
            value: self.value.unsigned_abs(),
        }
    }

    /// Returns the number of tick difference (positive or negative) that this TickDelta represents,
    /// rounding to the closest integer value
    pub fn to_i32(&self) -> i32 {
        self.value.round().to_num()
    }
}

impl Neg for TickDelta {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self {
            value: self.value.saturating_neg(),
        }
    }
}

impl Add for TickDelta {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            value: self.value.saturating_add(rhs.value),
        }
    }
}

impl Sub for TickDelta {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            value: self.value.saturating_sub(rhs.value),
        }
    }
}

/// A non-negative [`TickDelta`], as an interval travels on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
#[cfg_attr(feature = "reflect", derive(Reflect))]
#[cfg_attr(feature = "reflect", reflect(opaque))]
pub struct PositiveTickDelta {
    pub value: U32F32,
}

impl PositiveTickDelta {
    pub const fn lit(src: &str) -> Self {
        Self {
            value: U32F32::lit(src),
        }
    }
    pub fn tick_diff(&self) -> u32 {
        self.value.to_num::<u32>()
    }
    pub fn overstep(&self) -> Overstep {
        Overstep::new(self.value.wrapping_to_num())
    }

    /// The time these ticks take at `rate`, rounded to the nearest nanosecond.
    pub fn to_duration(&self, rate: TickRate) -> Duration {
        rate.duration_of(self.value)
    }

    /// The ticks `duration` spans at `rate`, exact to 2^-32 tick and saturated at the largest
    /// delta.
    pub fn from_duration(duration: Duration, rate: TickRate) -> Self {
        Self {
            value: rate.ticks_in(duration),
        }
    }
}

impl TryFrom<TickDelta> for PositiveTickDelta {
    type Error = TickRangeError;

    fn try_from(value: TickDelta) -> Result<Self, Self::Error> {
        value
            .value
            .checked_to_fixed()
            .map(|value| Self { value })
            .ok_or(TickRangeError)
    }
}

impl ToBytes for PositiveTickDelta {
    fn bytes_len(&self) -> usize {
        4 + self.overstep().bytes_len()
    }

    fn to_bytes(&self, buffer: &mut impl WriteInteger) {
        buffer.write_u32(self.tick_diff());
        self.overstep().to_bytes(buffer);
    }

    fn from_bytes(buffer: &mut Reader) -> Result<Self, SerializationError>
    where
        Self: Sized,
    {
        let tick_diff = buffer.read_u32()?;
        let overstep = Overstep::from_bytes(buffer)?;
        Ok(Self {
            value: tick_diff.to_fixed::<U32F32>() + U32F32::from(overstep.value),
        })
    }
}

#[cfg(test)]
mod tests;
