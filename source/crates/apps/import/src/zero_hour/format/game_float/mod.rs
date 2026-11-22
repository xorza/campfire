//! The game's `f32` values as numbers, read from their bits: the engine has no float conversion
//! of its own (07-determinism-core.md, No float converts).

use campfire_math::{Num, Rounding};
use campfire_values::Share;

use crate::zero_hour::format::error::FloatError;

/// The share `value` holds, to the nearest billionth a share holds; none below 0 or past 1.
pub(crate) fn to_share(value: f32) -> Option<Share> {
    if !(0.0..=1.0).contains(&value) {
        return None;
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a share from 0 to 1 is from 0 to a billion billionths"
    )]
    let billionths = (f64::from(value) * 1e9).round() as u32;
    Share::billionths(billionths)
}

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
mod tests;
