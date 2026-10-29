use core::fmt::Debug;
use core::time::Duration;

#[cfg(feature = "reflect")]
use bevy_reflect::Reflect;
use campfire_math::Num;
use campfire_net_bytes::Reader;
use campfire_net_bytes::SerializationError;
use campfire_net_bytes::ToBytes;
use campfire_net_bytes::WriteInteger;

use crate::tick_range_error::TickRangeError;

/// The fraction of the next tick a timeline has run, in `[0, 1)`, a `Num` of 24 fractional bits
/// (11-net-stack/time.md, One number type). It compares at its full precision and travels as
/// its top 8 bits.
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "reflect", derive(Reflect))]
#[cfg_attr(feature = "reflect", reflect(opaque))]
pub struct Overstep {
    value: Num,
}

impl Overstep {
    const WIRE_SHIFT: u32 = Num::FRAC_BITS - u8::BITS;

    /// The overstep whose 24 fractional bits are the low bits of `bits`.
    pub(crate) const fn from_fraction_bits(bits: u32) -> Self {
        Self {
            value: Num::from_bits((bits & ((1 << Num::FRAC_BITS) - 1)) as i64),
        }
    }

    pub const fn value(self) -> Num {
        self.value
    }

    /// The share of `timestep` that `overstep` ran, as Bevy's fixed time gives both, to the
    /// nearest 2^-24 from their nanoseconds, a half up, and at most 1 - 2^-24 (11-net-stack/time.md,
    /// Conversions are exact).
    pub fn of(overstep: Duration, timestep: Duration) -> Self {
        let (ran, tick) = (overstep.as_nanos(), timestep.as_nanos());
        debug_assert!(tick > 0, "a tick takes time");
        let steps = ((ran << (Num::FRAC_BITS + 1)) + tick) / (tick << 1);
        let most = (1 << Num::FRAC_BITS) - 1;
        Self::from_fraction_bits(u32::try_from(steps.min(most)).expect("the clamp fits u32"))
    }

    /// The overstep as an `f32`, which holds its 24 bits exactly.
    #[expect(clippy::cast_precision_loss, reason = "24 bits fit an f32's mantissa")]
    pub fn to_f32(self) -> f32 {
        self.value.to_bits() as f32 / 16_777_216.0
    }

    pub const fn from_u8(value: u8) -> Self {
        Self::from_fraction_bits((value as u32) << Self::WIRE_SHIFT)
    }

    /// The top 8 bits, a step of 1/256 rounded down.
    pub const fn to_u8(self) -> u8 {
        (self.value.to_bits().cast_unsigned() >> Self::WIRE_SHIFT) as u8
    }
}

/// An overstep is in `[0, 1)`.
impl TryFrom<Num> for Overstep {
    type Error = TickRangeError;

    fn try_from(value: Num) -> Result<Self, Self::Error> {
        if (Num::ZERO..Num::ONE).contains(&value) {
            Ok(Self { value })
        } else {
            Err(TickRangeError)
        }
    }
}

impl ToBytes for Overstep {
    fn bytes_len(&self) -> usize {
        1
    }

    fn to_bytes(&self, buffer: &mut impl WriteInteger) {
        buffer.write_u8(self.to_u8());
    }

    fn from_bytes(buffer: &mut Reader) -> Result<Self, SerializationError>
    where
        Self: Sized,
    {
        Ok(Self::from_u8(buffer.read_u8()?))
    }
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use crate::overstep::Overstep;
    use crate::tick_range_error::TickRangeError;
    use campfire_math::Num;

    /// 0.5 and 0.5 + 2^-24 differ, and order so, while both serialize to the byte 128: equality
    /// and order agree at the full precision, and only the wire quantizes.
    #[test]
    fn comparisons_use_the_full_precision() {
        let low = Overstep::try_from(Num::HALF).unwrap();
        let high = Overstep::try_from(Num::HALF + Num::EPSILON).unwrap();
        assert_ne!(low, high);
        assert!(low < high);
        assert_eq!((low.to_u8(), high.to_u8()), (128, 128));
        assert_eq!(Overstep::try_from(Num::ONE), Err(TickRangeError));
        assert_eq!(Overstep::try_from(-Num::EPSILON), Err(TickRangeError));
    }

    #[test]
    fn an_overstep_is_the_share_of_its_tick_to_one_in_2_to_the_24_and_travels_to_one_in_256() {
        // i/10 of a 100 ms tick is i · 2^24 / 10 steps to the nearest, and `to_u8` truncates that
        // to 1/256: floor(round(i · 2^24 / 10) / 2^16) / 256. A whole tick clamps to 1 - 2^-24.
        let tick = Duration::from_millis(100);
        let expected: [u8; 11] = [0, 25, 51, 76, 102, 128, 153, 179, 204, 230, 255];
        for (i, expected) in (0..=10_u8).zip(expected) {
            let quantized = Overstep::of(tick * u32::from(i) / 10, tick).to_u8();
            assert_eq!(quantized, expected, "{i} / 10");
            assert_eq!(
                Overstep::from_u8(quantized).to_f32(),
                f32::from(expected) / 256.0
            );
        }
        // A tenth is 1,677,721.6 steps, to the nearest 1,677,722; 1 ns of a tick of 2^25 ns is half
        // a step, a half up to 1.
        assert_eq!(
            Overstep::of(Duration::from_millis(10), tick).value(),
            Num::from_bits(1_677_722)
        );
        let half = Overstep::of(Duration::from_nanos(1), Duration::from_nanos(1 << 25));
        assert_eq!(half.value(), Num::from_bits(1));
        assert_eq!(Overstep::of(tick, tick).value(), Num::ONE - Num::EPSILON);
        assert_eq!(Overstep::of(Duration::ZERO, tick), Overstep::default());
    }
}
