use crate::num::{Num, SinCos, to_i64};
use crate::rounding::Rounding;
use crate::wide_division::WideDivision;

/// Fractional bits of the internal fixed point. Every intermediate of the kernels is at most 1 in
/// magnitude, so it fits `i64` and a product of two is a single 64×64 multiply.
const WIDE_BITS: u32 = 62;
const WIDE_ONE: i64 = 1 << WIDE_BITS;

/// Fractional bits of `PI_SCALED`.
const PI_BITS: u32 = 120;
/// π · 2¹²⁰, within `PI_ERROR` of the true value.
const PI_SCALED: u128 = machin_pi();
/// Bound on the truncation error of `machin_pi`: under 1000 units of 2⁻¹²⁰ by its term count.
const PI_ERROR: u128 = 1 << 12;
const PI_WIDE: i128 =
    Rounding::NearestEven.shift_right(PI_SCALED.cast_signed(), PI_BITS - WIDE_BITS);
const HALF_PI_WIDE: i128 =
    Rounding::NearestEven.shift_right(PI_SCALED.cast_signed(), PI_BITS - WIDE_BITS + 1);
const PI_MAGNITUDE: u64 = wide_magnitude(PI_WIDE);
const HALF_PI_MAGNITUDE: u64 = wide_magnitude(HALF_PI_WIDE);
/// 2/π · 2⁶², to find the quadrant with a multiply.
const TWO_OVER_PI: i64 = to_i64(Rounding::NearestEven.divide_in_const(1 << 124, HALF_PI_WIDE));
/// Fractional bits of π/2 for reducing an angle: with 101, `n · π/2` stays exact to 2⁻⁶² for
/// every quadrant count `n` of a `Num`, which is below 2³⁹.
const REDUCE_BITS: u32 = 101;
const HALF_PI_REDUCE: i128 =
    Rounding::NearestEven.shift_right(PI_SCALED.cast_signed(), PI_BITS - REDUCE_BITS + 1);

/// `sin_cos` looks up the nearest angle `k / 64`, which leaves a remainder of at most 1/128;
/// entries up to 51/64 cover π/4 with margin.
const SIN_COS_STEP_BITS: u32 = 6;
const SIN_COS_ENTRIES: usize = 52;
const SIN_TABLE: [i64; SIN_COS_ENTRIES] = sin_cos_table(true);
const COS_TABLE: [i64; SIN_COS_ENTRIES] = sin_cos_table(false);

/// `atan2` rotates by the nearest `atan(k / 32)`, which leaves a tangent of about 1/64 at most.
const ATAN_STEP_BITS: u32 = 5;
const ATAN_ENTRIES: usize = 33;
const ATAN_ANGLE: [i64; ATAN_ENTRIES] = atan_table(AtanColumn::Angle);
const ATAN_COS: [i64; ATAN_ENTRIES] = atan_table(AtanColumn::Cos);
const ATAN_SIN: [i64; ATAN_ENTRIES] = atan_table(AtanColumn::Sin);

const INV_2: i64 = WIDE_ONE / 2;
const INV_6: i64 = WIDE_ONE / 6;
const INV_24: i64 = WIDE_ONE / 24;
const INV_120: i64 = WIDE_ONE / 120;
const INV_720: i64 = WIDE_ONE / 720;

const INV_3: i64 = WIDE_ONE / 3;
const INV_5: i64 = WIDE_ONE / 5;
const INV_7: i64 = WIDE_ONE / 7;

/// π · 2^`frac_bits`, rounded to nearest. Compilation fails if `PI_ERROR` could move π across a
/// rounding midpoint.
pub(super) const fn pi_bits(frac_bits: u32) -> i64 {
    let shift = PI_BITS - frac_bits;
    let floor = PI_SCALED >> shift;
    let rest = PI_SCALED - (floor << shift);
    let half = 1 << (shift - 1);
    assert!(
        rest.abs_diff(half) > PI_ERROR,
        "π too close to a rounding midpoint"
    );
    let rounded = if rest > half { floor + 1 } else { floor };
    Num::from_wide_bits(rounded.cast_signed()).to_bits()
}

pub(super) const fn sin_cos(angle: Num) -> SinCos {
    let x = angle.to_bits();
    // The nearest quadrant count. Off by one near a quadrant boundary, it only moves the
    // remainder a hair past π/4, which the table still covers.
    let quadrant = (x as i128 * TWO_OVER_PI as i128 + (1 << 85)) >> 86;
    // x − quadrant · π/2 at 2⁻¹⁰¹. Both terms overflow `i128`, but their difference is below
    // 2¹⁰¹, so computing modulo 2¹²⁸ gives it exactly.
    let rest = (x as i128)
        .wrapping_shl(REDUCE_BITS - Num::FRAC_BITS)
        .wrapping_sub(quadrant.wrapping_mul(HALF_PI_REDUCE));
    let r = to_i64(rest >> (REDUCE_BITS - WIDE_BITS));

    let index = (r + (1 << (WIDE_BITS - SIN_COS_STEP_BITS - 1))) >> (WIDE_BITS - SIN_COS_STEP_BITS);
    let b = r - (index << (WIDE_BITS - SIN_COS_STEP_BITS));
    let entry = to_index(index.unsigned_abs());
    // The index's sign and the quadrant are as random as the angles, so each turns the values
    // by arithmetic, which takes no branch to mispredict.
    let sin_a = if index < 0 {
        -SIN_TABLE[entry]
    } else {
        SIN_TABLE[entry]
    };
    let cos_a = COS_TABLE[entry];

    // |b| ≤ 2⁻⁷: the first omitted terms, b⁷/5040 and b⁸/40320, are below 2⁻⁶¹.
    // Each product of the series has a factor below 2⁶⁰, b at most 2⁵⁵ or b² at most 2⁴⁸, so it
    // takes `mul_by_quadruple` with that factor times 4, as `mul` would give it.
    let (b4, b2) = (b << 2, mul_by_quadruple(b, b << 2));
    let b2_4 = b2 << 2;
    let sin_b = b - mul_by_quadruple(
        mul_by_quadruple(INV_6 - mul_by_quadruple(b2, INV_120 << 2), b2_4),
        b4,
    );
    let cos_b = WIDE_ONE
        - mul_by_quadruple(
            INV_2 - mul_by_quadruple(INV_24 - mul_by_quadruple(b2, INV_720 << 2), b2_4),
            b2_4,
        );
    let sin = mul(sin_a, cos_b) + mul(cos_a, sin_b);
    let cos = mul(cos_a, cos_b) - mul(sin_a, sin_b);

    // Quadrants 0 to 3 give (sin, cos), (cos, −sin), (−sin, −cos) and (−cos, sin): an odd one
    // swaps the two, the upper two negate the first, and the middle two the second.
    let quadrant = quadrant as i64 & 3;
    let (sin, cos) = if quadrant & 1 == 1 {
        (cos, sin)
    } else {
        (sin, cos)
    };
    let sin = if quadrant & 2 == 2 { -sin } else { sin };
    let cos = if (quadrant + 1) & 2 == 2 { -cos } else { cos };
    SinCos {
        sin: to_num_narrow(sin),
        cos: to_num_narrow(cos),
    }
}

pub(super) fn atan2(y: Num, x: Num) -> Num {
    let y_abs = y.to_bits().unsigned_abs();
    let x_abs = x.to_bits().unsigned_abs();
    if y_abs == 0 && x_abs == 0 {
        return Num::ZERO;
    }
    let steep = y_abs > x_abs;
    let (small, large) = if steep {
        (x_abs, y_abs)
    } else {
        (y_abs, x_abs)
    };

    // The table point nearest small/large, from operands cut to 32 bits: off by one at most,
    // which the series still covers.
    let shift = large.bit_width().saturating_sub(32);
    let (small_cut, large_cut) = (small >> shift, large >> shift);
    let entry = to_index(((small_cut << (ATAN_STEP_BITS + 1)) + large_cut) / (2 * large_cut));

    // Rotating (large, small) by −atan(k/32) leaves a vector whose tangent u is about 1/64 at most.
    let cos_k = i128::from(ATAN_COS[entry]);
    let sin_k = i128::from(ATAN_SIN[entry]);
    let x_rot = i128::from(large) * cos_k + i128::from(small) * sin_k;
    let y_rot = i128::from(small) * cos_k - i128::from(large) * sin_k;
    let normalize = x_rot
        .cast_unsigned()
        .bit_width()
        .saturating_sub(WIDE_BITS + 1);
    // |u| ≤ 2⁻⁶ and a hair, so its quotient at 2⁻⁶² fits 64 bits, as the divisor does.
    let rotated = to_i64(y_rot >> normalize);
    let quotient = to_i64(
        WideDivision::of(
            u128::from(rotated.unsigned_abs()) << WIDE_BITS,
            (x_rot >> normalize).cast_unsigned(),
        )
        .quotient
        .cast_signed(),
    );
    let u = if rotated < 0 { -quotient } else { quotient };

    // |u| ≤ 2⁻⁶ and a hair: the first omitted term, u⁹/9, is below 2⁻⁵⁴.
    // As in `sin_cos`, each product has a factor below 2⁶⁰: u at most 2⁵⁶ and a hair, u² 2⁵⁰.
    let (u4, u2) = (u << 2, mul_by_quadruple(u, u << 2));
    let u2_4 = u2 << 2;
    let atan_u = u - mul_by_quadruple(
        mul_by_quadruple(
            INV_3 - mul_by_quadruple(INV_5 - mul_by_quadruple(u2, INV_7 << 2), u2_4),
            u2_4,
        ),
        u4,
    );
    // The angle from the x axis to the nearer of (large, small), below π/4, is never negative,
    // nor are the angles past π/2 and π taken from it, each below π · 2⁶² < 2⁶⁴: its magnitude
    // rounds on `u64`, and a rounding to nearest, ties to even, gives the negation's negation.
    let near = ATAN_ANGLE[entry] + atan_u;
    debug_assert!(near >= 0);
    let mut angle = near.cast_unsigned();
    if steep {
        angle = HALF_PI_MAGNITUDE - angle;
    }
    if x.to_bits() < 0 {
        angle = PI_MAGNITUDE - angle;
    }
    let magnitude = to_num_magnitude(angle);
    Num::from_bits(if y.to_bits() < 0 {
        -magnitude
    } else {
        magnitude
    })
}

/// `mul(a, quadruple / 4)` for a `quadruple` that is 4 times a factor: `(a · 4b) / 2⁶⁴` rounds
/// down as `(a · b) / 2⁶²` does, so it is the high half of one product, with no shift.
const fn mul_by_quadruple(a: i64, quadruple: i64) -> i64 {
    ((a as i128 * quadruple as i128) >> 64) as i64
}

/// A product at 2⁻⁶² of two values at most 1 in magnitude; truncated, the error stays far below
/// the final rounding to 2⁻²⁴.
const fn mul(a: i64, b: i64) -> i64 {
    to_i64((a as i128 * b as i128) >> WIDE_BITS)
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "table indices are below 64"
)]
const fn to_index(value: u64) -> usize {
    debug_assert!(value < 64);
    value as usize
}

/// `to_num` of a magnitude within `u64`, its rounding on `u64`, as bits.
#[expect(clippy::cast_possible_wrap, reason = "a magnitude below 2⁶⁴ over 2³⁸")]
fn to_num_magnitude(magnitude: u64) -> i64 {
    const SHIFT: u32 = WIDE_BITS - Num::FRAC_BITS;
    let floor = magnitude >> SHIFT;
    let rest = u128::from(magnitude & ((1 << SHIFT) - 1));
    let up = Rounding::NearestEven.rounds_up(rest, 1 << (SHIFT - 1), rest == 0, floor & 1 == 1);
    (floor + u64::from(up)) as i64
}

/// `value`, from 0 to below 2⁶⁴, as a `u64`.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "checked from 0 to below 2⁶⁴"
)]
const fn wide_magnitude(value: i128) -> u64 {
    assert!(0 <= value && value < 1 << 64, "a magnitude below 2⁶⁴");
    value as u64
}

/// `to_num` of a value within `i64`, its rounding on `i64`.
const fn to_num_narrow(value: i64) -> Num {
    const SHIFT: u32 = WIDE_BITS - Num::FRAC_BITS;
    let floor = value >> SHIFT;
    let rest = (value & ((1 << SHIFT) - 1)).cast_unsigned() as u128;
    let up = Rounding::NearestEven.rounds_up(rest, 1 << (SHIFT - 1), rest == 0, floor & 1 == 1);
    Num::from_bits(floor + up as i64)
}

/// sin or cos of `k / 64` at 2⁻⁶², by the exact series.
const fn sin_cos_table(sine: bool) -> [i64; SIN_COS_ENTRIES] {
    let mut table = [0; SIN_COS_ENTRIES];
    let mut k = 0;
    while k < SIN_COS_ENTRIES {
        let angle = (k as i128) << (WIDE_BITS - SIN_COS_STEP_BITS);
        let angle_squared = exact_mul(angle, angle);
        table[k] = to_i64(if sine {
            taylor(angle, 1, angle_squared)
        } else {
            taylor(WIDE_ONE as i128, 0, angle_squared)
        });
        k += 1;
    }
    table
}

#[derive(Debug, Clone, Copy)]
enum AtanColumn {
    Angle,
    Cos,
    Sin,
}

/// atan(k/32), and the cos and sin of that angle, at 2⁻⁶², by the exact series.
const fn atan_table(column: AtanColumn) -> [i64; ATAN_ENTRIES] {
    let mut table = [0; ATAN_ENTRIES];
    let mut k = 0;
    while k < ATAN_ENTRIES {
        let tangent = (k as i128) << (WIDE_BITS - ATAN_STEP_BITS);
        let one = WIDE_ONE as i128;
        let root = ((one + exact_mul(tangent, tangent)).cast_unsigned() << WIDE_BITS)
            .isqrt()
            .cast_signed();
        table[k] = to_i64(match column {
            AtanColumn::Angle => atan_series(halve(halve(tangent))) << 2,
            AtanColumn::Cos => Rounding::NearestEven.divide_in_const(1 << (2 * WIDE_BITS), root),
            AtanColumn::Sin => Rounding::NearestEven.divide_in_const(tangent << WIDE_BITS, root),
        });
        k += 1;
    }
    table
}

/// π · 2¹²⁰ by Machin's formula, `π = 16·atan(1/5) − 4·atan(1/239)`.
const fn machin_pi() -> u128 {
    16 * atan_inverse(5) - 4 * atan_inverse(239)
}

/// atan(1/x) · 2¹²⁰ by its series, each term truncated.
const fn atan_inverse(x: u128) -> u128 {
    let x_squared = x * x;
    let mut power = (1 << PI_BITS) / x;
    let mut sum = power;
    let mut odd = 1;
    let mut subtract = true;
    while power != 0 {
        power /= x_squared;
        odd += 2;
        let term = power / odd;
        sum = if subtract { sum - term } else { sum + term };
        subtract = !subtract;
    }
    sum
}

/// The sum of alternating Taylor terms from `first`, the power `first_power` of `r`; each term is
/// the previous one times −r² / ((p + 1)(p + 2)).
const fn taylor(first: i128, first_power: i128, r_squared: i128) -> i128 {
    let mut sum = first;
    let mut term = first;
    let mut power = first_power;
    while term != 0 {
        term = -(exact_mul(term, r_squared) / ((power + 1) * (power + 2)));
        sum += term;
        power += 2;
    }
    sum
}

const fn atan_series(u: i128) -> i128 {
    let u_squared = exact_mul(u, u);
    let mut sum = u;
    let mut power = u;
    let mut odd = 1;
    while power != 0 {
        power = -exact_mul(power, u_squared);
        odd += 2;
        sum += power / odd;
    }
    sum
}

/// `t / (1 + √(1 + t²))`, the argument whose arctangent is half that of `t`.
const fn halve(t: i128) -> i128 {
    let root = ((WIDE_ONE as i128 + exact_mul(t, t)).cast_unsigned() << WIDE_BITS)
        .isqrt()
        .cast_signed();
    Rounding::NearestEven.divide_in_const(t << WIDE_BITS, WIDE_ONE as i128 + root)
}

/// A rounded product at 2⁻⁶², for building the tables.
const fn exact_mul(a: i128, b: i128) -> i128 {
    Rounding::NearestEven.shift_right(a * b, WIDE_BITS)
}

#[cfg(test)]
mod tests;
