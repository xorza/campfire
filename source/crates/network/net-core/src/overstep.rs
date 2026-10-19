use crate::tick_range_error::TickRangeError;
#[cfg(feature = "reflect")]
use bevy_reflect::Reflect;
use campfire_math::Num;
use campfire_net_bytes::ReadInteger;
use campfire_net_bytes::Reader;
use campfire_net_bytes::SerializationError;
use campfire_net_bytes::ToBytes;
use campfire_net_bytes::WriteInteger;
use core::fmt::Debug;

/// The fraction of the next tick a timeline has run, in `[0, 1)`, a `Num` of 24 fractional bits
/// (12-lightyear-in-tree.md, One number type). It compares at its full precision and travels as
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

    /// The nearest overstep to `value`, which Bevy's fixed time gives as an `f32`: an `f32` in
    /// `[0, 1)` times 2^24 is exact, and only its rounding to a whole step and the clamp to
    /// `[0, 1 - 2^-24]` move it.
    pub fn from_f32(value: f32) -> Self {
        #[expect(
            clippy::cast_sign_loss,
            reason = "the clamp keeps the steps at 0 or more, and a NaN is 0"
        )]
        let bits = (value * 16_777_216.0).round().clamp(0.0, 16_777_215.0) as u32;
        Self::from_fraction_bits(bits)
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
    fn an_overstep_quantizes_to_one_in_2_to_the_24_and_truncates_to_one_in_256() {
        // `from_f32` rounds to the nearest 2^-24, and `to_u8` truncates that to 1/256: i / 10
        // maps to floor(round(i / 10 · 2^24) / 2^16) / 256, and 1.0 clamps to 1 - 2^-24.
        let expected: [u8; 11] = [0, 25, 51, 76, 102, 128, 153, 179, 204, 230, 255];
        for (i, expected) in (0..=10_u8).zip(expected) {
            let quantized = Overstep::from_f32(f32::from(i) / 10.0).to_u8();
            assert_eq!(quantized, expected, "{i} / 10");
            assert_eq!(
                Overstep::from_u8(quantized).to_f32(),
                f32::from(expected) / 256.0
            );
        }
        // 0.1 as an `f32` is 13,421,773 · 2^-27, 1,677,721.625 steps of 2^-24, to the nearest
        // 1,677,722.
        assert_eq!(Overstep::from_f32(0.1).value(), Num::from_bits(1_677_722));
        assert_eq!(Overstep::from_f32(1.0).value(), Num::ONE - Num::EPSILON);
        assert_eq!(Overstep::from_f32(-0.5), Overstep::default());
        assert_eq!(Overstep::from_f32(f32::NAN), Overstep::default());
    }
}
