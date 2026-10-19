use crate::coordinate;
use crate::direction::FRACTION;

/// The divisor of a unit's axes, a root `d` from 2⁶² to 2⁶³, as the square it is taken from
/// has 125 or 126 bits, and what each axis's quotient shares: the float reciprocal the root's
/// estimate took, within 2⁻⁵⁰·⁶ of `1 / d`, so the three quotients take no division and their
/// estimates wait on no integer step of the root, and `scale`, it times 2^(`shift` − 11),
/// exactly so of `2^(shift − 11) / d`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct UnitDivisor {
    root: u64,
    shift: u32,
    reciprocal: f64,
    scale: f64,
}

impl UnitDivisor {
    /// The power of two each estimate counts in: a quotient over it stays below 2⁵².
    const COARSE: u32 = 11;

    #[expect(
        clippy::float_arithmetic,
        reason = "a float reciprocal that `unit` corrects exactly"
    )]
    #[inline]
    pub(crate) fn new(root: u64, half_reciprocal: f64, shift: u32) -> Self {
        debug_assert!((1 << 62..=1 << 63).contains(&root));
        debug_assert!((FRACTION..=FRACTION + 62).contains(&shift));
        let power = f64::from_bits(u64::from(1023 + 1 + shift - Self::COARSE) << 52);
        Self {
            root,
            shift,
            reciprocal: 2.0 * half_reciprocal,
            scale: half_reciprocal * power,
        }
    }

    /// `axis · 2^shift / d`, rounded to nearest, ties to even, for an axis at most the length
    /// whose square's root is `d` at that scale, so a quotient `Q` at most 2⁶² and a hair. The
    /// magnitude, below 2⁴⁷, is exact in f64, and its product with `scale`, `Q / 2¹¹` within
    /// 2⁻⁵⁰·⁶ of itself, so within 1.4, rounds once to an integer below 2⁵², which fills the low
    /// bits of 2⁵²: `q`, it times 2¹¹, lies within 2¹¹ · 1.9 of `Q`. The rest `N − q · d` of the
    /// numerator `N` is then below 2⁷⁵ in magnitude, within `i128`, and its ratio to `d`, its
    /// float times `reciprocal` with one rounding, lies within 2⁻³⁷ of `Q − q`, below 2¹²: its
    /// nearest integer takes `q` to `m`, the nearest to `Q` or one off it when `Q` lies within
    /// 2⁻³⁷ of a half. The rest `N − m · d`, below `d · (½ + 2⁻³⁷)` in magnitude, fits an `i64`,
    /// so its low 64 bits are exact, and against half of `d` one step corrects `m`, rarely taken.
    /// `m` stays within 2⁶² and a hair, so no step can wrap.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        reason = "a float estimate that the integer steps correct exactly"
    )]
    #[inline(always)]
    pub(crate) fn unit(self, axis: i128) -> i64 {
        /// 2⁵², whose last place is 1: a float from 0 to below 2⁵² added to it rounds to the
        /// nearest integer, ties to even, which fills its low bits.
        const TWO_POW_52: f64 = 4_503_599_627_370_496.0;
        /// 1.5 · 2⁵², whose last place is 1: a float within 2⁵¹ of 0 added to it rounds to the
        /// nearest integer, which fills its low bits.
        const SIGNED_52: f64 = 6_755_399_441_055_744.0;
        /// 2³², exact in f64.
        const TWO_POW_32: f64 = 4_294_967_296.0;
        debug_assert!(axis.unsigned_abs() < coordinate::WIDEST.unsigned_abs());
        let magnitude = axis.unsigned_abs() as u64;
        let numerator = u128::from(magnitude) << self.shift;
        let coarse = (magnitude.cast_signed() as f64).mul_add(self.scale, TWO_POW_52);
        let estimate = (coarse.to_bits() - TWO_POW_52.to_bits()) << Self::COARSE;
        let rest = numerator
            .wrapping_sub(u128::from(estimate) * u128::from(self.root))
            .cast_signed();
        let float_rest = ((rest >> 32) as i64 as f64).mul_add(TWO_POW_32, f64::from(rest as u32));
        let step = (float_rest.mul_add(self.reciprocal, SIGNED_52).to_bits())
            .wrapping_sub(SIGNED_52.to_bits())
            .cast_signed();
        let mut nearest = estimate.wrapping_add_signed(step);
        let rest = (numerator as u64)
            .wrapping_sub(nearest.wrapping_mul(self.root))
            .cast_signed();
        // `2 · rest` against `d`, as `rest` against half of `d`: a tie only for an even `d`.
        let half = (self.root >> 1).cast_signed();
        let tie = self.root & 1 == 0 && nearest & 1 == 1;
        if rest > half || (rest == half && tie) {
            nearest = nearest.wrapping_add(1);
        } else if rest < -half || (rest == -half && tie) {
            nearest = nearest.wrapping_sub(1);
        }
        // All ones for a negative axis, so the xor and the subtraction negate.
        let sign = (axis >> 127) as i64;
        (nearest.cast_signed() ^ sign).wrapping_sub(sign)
    }
}
