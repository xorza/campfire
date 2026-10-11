use crate::num::Num;
use crate::vec3::step_moves::StepMoves;

/// Each move from a float estimate and its exact correction, for a distance below 2⁵⁰; past it,
/// by its division. ARM has no 128-by-64-bit division, and the estimate measured 37 % faster
/// than the divisions on an M2.
#[inline]
pub(super) fn moves(moves: StepMoves, offsets: [Num; 3]) -> [Num; 3] {
    if moves.distance >= FLOAT_END {
        return offsets.map(|offset| moves.divided(offset));
    }
    let ratio = ratio(moves.step, moves.distance);
    offsets.map(|offset| estimated(offset, moves.step, moves.distance, ratio))
}

/// The end of the distances, in bits, whose moves take the estimate: 2⁵⁰.
const FLOAT_END: i64 = 1 << 50;
/// 2⁵², whose last place is 1: a float from 0 to below 2⁵² added to it rounds to the nearest
/// integer, ties to even, which fills its low bits.
const TWO_POW_52: f64 = 4_503_599_627_370_496.0;

/// `step / distance`, the ratio of each move to its offset, rounded once; both are exact in f64
/// below 2⁵⁰.
#[expect(
    clippy::cast_precision_loss,
    clippy::float_arithmetic,
    reason = "an estimate that `step_component` corrects exactly"
)]
fn ratio(step: i64, distance: i64) -> f64 {
    step as f64 / distance as f64
}

/// `offset · step / distance` rounded to nearest, ties to even, from `ratio`, for a distance below 2⁵⁰ and
/// at least `step` and `|offset|`, so the quotient is at most `|offset|`. Each of the three is
/// below 2⁵⁰ and converts exactly, and the estimate rounds twice, by at most 2⁻⁵³ of itself
/// each, so it lies within 2⁻²⁵ of the quotient's magnitude; its nearest integer `m` is the
/// quotient's or one off it, when the quotient lies within that of a half. The rest
/// `|offset| · step − m · distance` is below the distance in magnitude, so its low 64 bits,
/// wrapped, are the rest, and every step runs on `u64`: the nearest, ties to even, has
/// `−distance ≤ 2 · rest ≤ distance`, the bound taken where `m` is even.
#[expect(
    clippy::cast_precision_loss,
    clippy::float_arithmetic,
    reason = "an estimate that the integer steps correct exactly"
)]
#[inline]
fn estimated(offset: Num, step: i64, distance: i64, ratio: f64) -> Num {
    let magnitude = offset.to_bits().unsigned_abs();
    let estimate = magnitude.cast_signed() as f64 * ratio;
    let mut moved = (estimate + TWO_POW_52).to_bits() - TWO_POW_52.to_bits();
    let distance = distance.cast_unsigned();
    let rest = (magnitude.wrapping_mul(step.cast_unsigned()))
        .wrapping_sub(moved.wrapping_mul(distance))
        .cast_signed();
    let (twice, reach) = (rest.wrapping_mul(2), distance.cast_signed());
    let odd = moved & 1 == 1;
    if twice > reach || (twice == reach && odd) {
        moved += 1;
    } else if twice < reach.wrapping_neg() || (twice == reach.wrapping_neg() && odd) {
        moved -= 1;
    }
    // All ones for a negative offset, so the xor and the subtraction negate, with no branch on
    // a random sign.
    let sign = offset.to_bits() >> 63;
    Num::from_bits((moved.cast_signed() ^ sign) - sign)
}
