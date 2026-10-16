//! The game's `f32` values as numbers, read from their bits: the engine has no float conversion
//! of its own (design 09, D7).

use campfire_math::{Num, Rounding};

use crate::zero_hour::format::error::FloatError;

/// `value` exactly where a step of 2⁻²⁴ holds it, as it holds every value of at least 2⁻¹ that
/// fits, and below rounded once to nearest, ties to even.
pub(crate) fn to_num(value: f32) -> Result<Num, FloatError> {
    if !value.is_finite() {
        return Err(FloatError::NotFinite);
    }
    let bits = value.to_bits();
    let exponent = i32::try_from((bits >> 23) & 0xFF).expect("eight bits");
    let fraction = i128::from(bits & 0x7F_FFFF);
    let (mantissa, power) = if exponent == 0 {
        (fraction, -149)
    } else {
        (fraction | 0x80_0000, exponent - 150)
    };
    // The value is `mantissa · 2^power`, so its bits are `mantissa · 2^(power + 24)`: a left
    // shift below 64 that fits an `i64` or none, or a right shift of at most 125.
    let scaled = power + Num::FRAC_BITS.cast_signed();
    let magnitude = if scaled >= 0 {
        if scaled >= 64 {
            return Err(FloatError::OutOfRange);
        }
        mantissa << scaled
    } else {
        Rounding::NearestEven.shift_right(mantissa, scaled.unsigned_abs())
    };
    let bits = if value.is_sign_negative() {
        -magnitude
    } else {
        magnitude
    };
    let bits = i64::try_from(bits).map_err(|_| FloatError::OutOfRange)?;
    Ok(Num::from_bits(bits))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_float_converts_exactly_or_rounds_once_below_a_step() {
        let num = |value: f32| to_num(value).map(Num::to_bits);
        // A float of sign `negative`, exponent `power` and fraction `fraction`, by its bits.
        let float = |negative: bool, power: i32, fraction: u32| {
            let exponent = u32::try_from(power + 127).unwrap();
            f32::from_bits((u32::from(negative) << 31) | (exponent << 23) | fraction)
        };
        // 1234.5677 in `f32` is 1234.5677490234375, 20,712,609,792 steps of 2⁻²⁴; −0.625,
        // 1.25 · 2⁻¹ below 0, is 10,485,760 steps below.
        assert_eq!(num(1234.5677), Ok(20_712_609_792));
        assert_eq!(num(float(true, -1, 0x20_0000)), Ok(-10_485_760));
        // 2⁻²⁵ is half a step and rounds to the even 0; 1.5 · 2⁻²⁴ rounds to the even 2;
        // 1.25 · 2⁻²⁴ to 1; −1.5 · 2⁻²⁴ to −2.
        assert_eq!(num(float(false, -25, 0)), Ok(0));
        assert_eq!(num(float(false, -24, 0x40_0000)), Ok(2));
        assert_eq!(num(float(false, -24, 0x20_0000)), Ok(1));
        assert_eq!(num(float(true, -24, 0x40_0000)), Ok(-2));
        // The smallest normal float, the smallest subnormal, and −0 are 0.
        assert_eq!(num(f32::MIN_POSITIVE), Ok(0));
        assert_eq!(num(f32::from_bits(1)), Ok(0));
        assert_eq!(num(f32::from_bits(1 << 31)), Ok(0));
        // (2²⁴ − 1) · 2¹⁵, the largest float below 2³⁹, is 2⁶³ − 2³⁹ steps; 2³⁹ is past a `Num`,
        // and −2³⁹, its least, is not.
        assert_eq!(
            num(float(false, 38, 0x7F_FFFF)),
            Ok(i64::MAX - (1 << 39) + 1)
        );
        assert_eq!(num(float(false, 39, 0)), Err(FloatError::OutOfRange));
        assert_eq!(num(float(true, 39, 0)), Ok(i64::MIN));
        assert_eq!(num(1e12), Err(FloatError::OutOfRange));
        assert_eq!(num(f32::MAX), Err(FloatError::OutOfRange));
        assert_eq!(num(f32::NAN), Err(FloatError::NotFinite));
        assert_eq!(num(f32::NEG_INFINITY), Err(FloatError::NotFinite));
    }
}
