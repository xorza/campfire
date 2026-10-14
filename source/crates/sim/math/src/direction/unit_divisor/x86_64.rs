use crate::direction::FRACTION;
use crate::wide_division::WideDivision;

/// The divisor of a unit's axes, a root `d` from 2⁶² to 2⁶³, as the square it is taken from
/// has 125 or 126 bits, and the shift of each axis.
#[derive(Debug, Clone, Copy)]
pub(crate) struct UnitDivisor {
    root: u64,
    shift: u32,
}

impl UnitDivisor {
    /// The divisor of `root`, for axes shifted by `shift`; a division takes no reciprocal, so
    /// the root estimate's goes unread.
    #[inline]
    pub(crate) fn new(root: u64, _half_reciprocal: f64, shift: u32) -> UnitDivisor {
        debug_assert!((1 << 62..=1 << 63).contains(&root));
        debug_assert!((FRACTION..=FRACTION + 62).contains(&shift));
        UnitDivisor { root, shift }
    }

    /// `axis · 2^shift / d`, rounded to nearest, ties to even, for an axis at most the length
    /// whose square's root is `d` at that scale, so a quotient at most 2⁶² and a hair: one
    /// division of 128 bits by 64. The rest is below `d`, at most 2⁶³, so twice it fits `u64`.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "an axis below 2⁴⁷ and a quotient below 2⁶³"
    )]
    #[inline(always)]
    pub(crate) fn unit(self, axis: i128) -> i64 {
        debug_assert!(axis.unsigned_abs() < 1 << 47);
        let magnitude = u128::from(axis.unsigned_abs() as u64) << self.shift;
        let WideDivision { quotient, rest } = WideDivision::of(magnitude, u128::from(self.root));
        let (quotient, twice) = (quotient as u64, (rest as u64) << 1);
        let up = twice > self.root || (twice == self.root && quotient & 1 == 1);
        let nearest = quotient.wrapping_add(u64::from(up)).cast_signed();
        // All ones for a negative axis, so the xor and the subtraction negate.
        let sign = (axis >> 127) as i64;
        (nearest ^ sign).wrapping_sub(sign)
    }
}
