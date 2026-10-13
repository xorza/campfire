use crate::u256::U256;

#[cfg(feature = "internals")]
pub(crate) mod bench;

/// An unsigned integer of 384 bits, the exact product of a `U256` and a `u128`: the most
/// significant limb first, so the derived order compares numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct U384 {
    high: u128,
    middle: u128,
    low: u128,
}

impl U384 {
    /// The exact product `a × b`; an `a` within 128 bits, as most are, takes one product.
    pub const fn product(a: U256, b: u128) -> U384 {
        let [high, middle, low] = a.wide_product(b);
        U384 { high, middle, low }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn products_order_by_value_at_384_bits() {
        let full = U256::from_halves(u128::MAX, u128::MAX);
        let p = U384::product;
        // (2²⁵⁶ − 1)(2¹²⁸ − 1) = 2³⁸⁴ − 2²⁵⁶ − 2¹²⁸ + 1 = (2¹²⁸ − 2) · 2²⁵⁶ + (2¹²⁸ − 1) · 2¹²⁸ + 1;
        // it passes (2²⁵⁶ − 1)(2¹²⁸ − 2) by 2²⁵⁶ − 1.
        assert_eq!(
            p(full, u128::MAX),
            U384 {
                high: u128::MAX - 1,
                middle: u128::MAX,
                low: 1
            }
        );
        assert!(p(full, u128::MAX) > p(full, u128::MAX - 1));
        // 2²⁵⁵ · 1 is 2¹²⁸ · 2¹²⁷, and below 2¹²⁸ · (2¹²⁷ + 1); 0 · 0 is 0 · 1.
        let (top, two_128) = (U256::from_halves(1 << 127, 0), U256::from_halves(1, 0));
        assert_eq!(p(top, 1), p(two_128, 1 << 127));
        assert!(p(top, 1) < p(two_128, (1 << 127) + 1));
        assert_eq!(p(U256::ZERO, 0), p(U256::ZERO, 1));
        // Within 128 bits, the product of one: (2¹²⁸ − 1)² passes (2¹²⁸ − 1)(2¹²⁸ − 2) in the
        // low limb too; 2¹²⁷ · 2 is 2 · 2¹²⁷, past (2¹²⁸ − 1) · 1 in the middle limb alone;
        // 2 · 7 = 14 lies below 3 · 5 = 15.
        let max = U256::from_u128(u128::MAX);
        assert!(p(max, u128::MAX) > p(max, u128::MAX - 1));
        let (half, two) = (U256::from_u128(1 << 127), U256::from_u128(2));
        assert_eq!(p(half, 2), p(two, 1 << 127));
        assert!(p(half, 2) > p(max, 1));
        assert!(p(two, 7) < p(U256::from_u128(3), 5));
        assert_eq!(p(U256::from_u128(7), 2), p(two, 7));
    }
}
