use crate::num::Num;
use crate::u256::U256;

/// A sum of products of raw values, exact at any size: what its positive products add, and what
/// its negative ones take away, each as a magnitude.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProductSum {
    positive: U256,
    negative: U256,
}

impl ProductSum {
    pub const ZERO: ProductSum = ProductSum {
        positive: U256::ZERO,
        negative: U256::ZERO,
    };

    /// Adds `a × b`. Each product is below 2¹⁹¹, so a sum of fewer than 2⁶⁴ never passes 256
    /// bits.
    pub const fn add(&mut self, a: i64, b: i128) {
        let product = U256::product(a.unsigned_abs() as u128, b.unsigned_abs());
        let side = if (a < 0) == (b < 0) {
            &mut self.positive
        } else {
            &mut self.negative
        };
        *side = side.checked_add(product).expect("fewer than 2⁶⁴ products");
    }

    /// The number nearest to the sum ÷ 2²⁴, rounded once, ties to even, and at the end of the
    /// range of numbers when past it.
    pub fn saturating_num(self) -> Num {
        if let Some(above) = self.positive.checked_sub(self.negative) {
            let bits = above.round_shr(Num::FRAC_BITS);
            let bits = bits.and_then(|bits| i64::try_from(bits).ok());
            return bits.map_or(Num::MAX, Num::from_bits);
        }
        let below = self
            .negative
            .checked_sub(self.positive)
            .expect("one side is the larger");
        let bits = below.round_shr(Num::FRAC_BITS);
        let bits = bits.and_then(|bits| u64::try_from(bits).ok());
        let bits = bits.and_then(|bits| 0_i64.checked_sub_unsigned(bits));
        bits.map_or(Num::MIN, Num::from_bits)
    }
}

#[cfg(test)]
mod tests;
