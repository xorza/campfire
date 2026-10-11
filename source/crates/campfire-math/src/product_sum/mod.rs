use crate::num::Num;
use crate::rounding::Rounding;
use crate::u256::U256;

#[cfg(feature = "bench")]
pub(crate) mod bench;

/// A sum of products of raw values, exact at any size: a 256-bit two's complement number, its
/// high half first. Each product is below 2¹⁹⁰ in magnitude, so a sum of fewer than 2⁶⁴ never
/// leaves the range, and each add is the same arithmetic whatever the signs, with no branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProductSum {
    high: u128,
    low: u128,
}

impl ProductSum {
    pub const ZERO: ProductSum = ProductSum { high: 0, low: 0 };

    /// Adds `a × b`, as `a × b_low + a × b_high · 2⁶⁴` for `b`'s halves, each a product that
    /// fits `i128`: `b_low` below 2⁶⁴ and `b_high` within `i64`. The low product's high half joins
    /// the high product first, within `i128` as that product is within 2¹²⁶ and the half 2⁶³, so
    /// the whole product is its low product's low half and that middle sum, one 256-bit addend.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the halves of b, each within its type"
    )]
    pub const fn add(&mut self, a: i64, b: i128) {
        let a = a as i128;
        let low_part = a.wrapping_mul(b as u64 as i128);
        let high_part = a.wrapping_mul((b >> 64) as i64 as i128);
        let middle = high_part.wrapping_add(low_part >> 64);
        self.add_wide(
            (low_part as u64 as u128) | (middle << 64).cast_unsigned(),
            (middle >> 64).cast_unsigned(),
        );
    }

    /// Adds the 256-bit two's complement number `high · 2¹²⁸ + low`, wrapping, which the bound
    /// of the sum keeps exact.
    const fn add_wide(&mut self, low: u128, high: u128) {
        let (sum, carry) = self.low.overflowing_add(low);
        self.low = sum;
        self.high = self.high.wrapping_add(high).wrapping_add(carry as u128);
    }

    /// The number nearest to the sum ÷ 2²⁴, rounded once, ties to even, and at the end of the
    /// range of numbers when past it.
    pub fn saturating_num(self) -> Num {
        if self.high.cast_signed() >= 0 {
            let above = U256::from_halves(self.high, self.low);
            let bits = above.shr_rounded(Num::FRAC_BITS, Rounding::NearestEven);
            let bits = bits.and_then(|bits| i64::try_from(bits).ok());
            return bits.map_or(Num::MAX, Num::from_bits);
        }
        // The magnitude: the complement plus one.
        let (low, carry) = (!self.low).overflowing_add(1);
        let below = U256::from_halves((!self.high).wrapping_add(u128::from(carry)), low);
        let bits = below.shr_rounded(Num::FRAC_BITS, Rounding::NearestEven);
        let bits = bits.and_then(|bits| u64::try_from(bits).ok());
        let bits = bits.and_then(|bits| 0_i64.checked_sub_unsigned(bits));
        bits.map_or(Num::MIN, Num::from_bits)
    }
}

#[cfg(test)]
mod tests;
