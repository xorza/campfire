#[cfg(feature = "reflect")]
use bevy_reflect::Reflect;
use campfire_net_bytes::ReadInteger;
use campfire_net_bytes::Reader;
use campfire_net_bytes::SerializationError;
use campfire_net_bytes::ToBytes;
use campfire_net_bytes::WriteInteger;
use core::fmt::Debug;
use fixed::types::{U0F8, U0F16};

// quantization error for u8 is about 0.2%, for u16 is 0.0008%
/// Overstep fraction towards the next tick
///
/// Represents a value between 0.0 and 1.0 that indicates progress towards the next tick, compared
/// and ordered at its full precision. Serializes to a u8 value for network transmission.
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "reflect", derive(Reflect))]
#[cfg_attr(feature = "reflect", reflect(opaque))]
pub struct Overstep {
    pub(crate) value: U0F16,
}

impl Overstep {
    pub const fn new(value: U0F16) -> Self {
        Self { value }
    }
    pub const fn lit(src: &str) -> Self {
        Self {
            value: U0F16::lit(src),
        }
    }

    pub const fn value(&self) -> U0F16 {
        self.value
    }

    pub fn from_f32(value: f32) -> Self {
        Self::new(U0F16::saturating_from_num(value))
    }

    pub fn to_f32(&self) -> f32 {
        self.value.into()
    }

    pub fn from_u8(value: u8) -> Self {
        Self::new(U0F8::from_bits(value).into())
    }

    pub fn to_u8(&self) -> u8 {
        self.value.to_num::<U0F8>().to_bits()
    }
}

impl ToBytes for Overstep {
    fn bytes_len(&self) -> usize {
        1 // we only need 1 byte for a u8
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

impl From<f32> for Overstep {
    fn from(value: f32) -> Self {
        Self::from_f32(value)
    }
}

impl From<Overstep> for f32 {
    fn from(overstep: Overstep) -> Self {
        overstep.to_f32()
    }
}

#[cfg(test)]
mod tests {
    use crate::overstep::Overstep;
    use fixed::types::U0F16;

    /// 0.5 and 0.5 + 2^-16 differ, and order so, while both serialize to the byte 128: equality
    /// and order agree at the full precision, and only the wire quantizes.
    #[test]
    fn comparisons_use_the_full_precision() {
        let low = Overstep::lit("0.5");
        let high = Overstep::new(U0F16::from_bits(low.value().to_bits() + 1));
        assert_ne!(low, high);
        assert!(low < high);
        assert_eq!((low.to_u8(), high.to_u8()), (128, 128));
    }

    #[test]
    fn overstep_quantizes_to_one_in_65536_and_truncates_to_one_in_256() {
        // `from_f32` rounds to the nearest 1/65536 (saturating at 65535/65536), and `to_u8`
        // truncates that to 1/256: i / 10 maps to floor(round(i / 10 * 65536) / 256) / 256.
        let expected: [u8; 11] = [0, 25, 51, 76, 102, 128, 153, 179, 204, 230, 255];
        for (i, expected) in (0..=10_u8).zip(expected) {
            let quantized = Overstep::from_f32(f32::from(i) / 10.0).to_u8();
            assert_eq!(quantized, expected, "{i} / 10");
            assert_eq!(
                Overstep::from_u8(quantized).to_f32(),
                f32::from(expected) / 256.0
            );
        }
    }
}
