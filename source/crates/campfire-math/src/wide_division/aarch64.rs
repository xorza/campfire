use crate::wide_division::WideDivision;

/// 2⁶⁴, exact in f64.
const TWO_POW_64: f64 = 18_446_744_073_709_551_616.0;
/// 2³², exact in f64.
const TWO_POW_32: f64 = 4_294_967_296.0;

/// The quotient of a divisor and a quotient that fit 64 bits, from a float estimate, a step by the float ratio of its rest, and an exact
/// correction, with one float reciprocal for both. The numerator converts as two halves with one
/// rounding, the divisor with one, the reciprocal and the product round once each, so the
/// estimate lies within 2⁻⁵⁰ of the quotient, below 2⁶⁴: within 2¹⁴, and its integer part `q`,
/// held below 2⁶⁴, within 2¹⁵. The rest `numerator − q · divisor` is below 2¹⁵ · divisor in
/// magnitude, so within `i128`, and its ratio to the divisor, as floats, lies within 2⁻³⁵ of the
/// true one: its floor, below 2¹⁶ in magnitude, moves `q` to the quotient or one off it, which
/// the last step corrects, rarely taken.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::float_arithmetic,
    reason = "a float estimate that the integer steps correct exactly"
)]
#[inline]
fn narrow_quotient(numerator: u128, divisor: u64) -> WideDivision {
    let reciprocal = 1.0 / divisor as f64;
    let float_numerator =
        ((numerator >> 64) as u64 as f64).mul_add(TWO_POW_64, numerator as u64 as f64);
    let estimate = (float_numerator * reciprocal) as u64;
    let rest = numerator
        .wrapping_sub(u128::from(estimate) * u128::from(divisor))
        .cast_signed();
    let float_rest = ((rest >> 32) as i64 as f64).mul_add(TWO_POW_32, f64::from(rest as u32));
    let step = (float_rest * reciprocal).floor() as i64;
    let mut quotient = i128::from(estimate) + i128::from(step);
    let mut rest = rest - i128::from(step) * i128::from(divisor);
    let wide_divisor = i128::from(divisor);
    if rest < 0 {
        quotient -= 1;
        rest += wide_divisor;
    } else if rest >= wide_divisor {
        quotient += 1;
        rest -= wide_divisor;
    }
    WideDivision {
        quotient: quotient as u128,
        rest: rest as u128,
    }
}

/// A divisor and a quotient that fit 64 bits take the estimate, others Rust's `u128` division.
#[expect(clippy::cast_possible_truncation, reason = "the divisor fits 64 bits")]
#[inline]
pub(super) fn divide(numerator: u128, divisor: u128) -> WideDivision {
    if divisor >> 64 == 0 && numerator >> 64 < divisor {
        narrow_quotient(numerator, divisor as u64)
    } else {
        WideDivision {
            quotient: numerator / divisor,
            rest: numerator % divisor,
        }
    }
}
