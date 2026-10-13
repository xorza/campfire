use crate::u256::U256;

#[cfg(feature = "internals")]
pub(crate) mod bench;

/// A signed integer of 256 bits, in two's complement, the exact product of two `i128`s: the high
/// half, signed, before the low, so the derived order compares numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct I256 {
    high: i128,
    low: u128,
}

impl I256 {
    /// The exact product `a × b`: the product of the magnitudes, below 2²⁵⁴, with the sign.
    pub const fn product(a: i128, b: i128) -> I256 {
        let magnitude = U256::product(a.unsigned_abs(), b.unsigned_abs());
        let product = I256 {
            high: magnitude.high().cast_signed(),
            low: magnitude.low(),
        };
        if (a < 0) == (b < 0) {
            product
        } else {
            product.negated()
        }
    }

    /// `-self`, which no product overflows.
    const fn negated(self) -> I256 {
        let (low, carry) = (!self.low).overflowing_add(1);
        I256 {
            high: (!self.high).wrapping_add(carry as i128),
            low,
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn products_order_by_value_with_their_signs() {
        let p = I256::product;
        // Within i128 the high half is the sign's: 0 or −1.
        assert_eq!(p(6, 7), I256 { high: 0, low: 42 });
        assert_eq!(
            p(-6, 7),
            I256 {
                high: -1,
                low: 42_u128.wrapping_neg()
            }
        );
        assert_eq!(p(-6, -7), p(6, 7));
        assert_eq!(p(0, -5), I256 { high: 0, low: 0 });
        // The ends: (−2¹²⁷)² = 2²⁵⁴, and −2¹²⁷ · (2¹²⁷ − 1) = −2²⁵⁴ + 2¹²⁷.
        assert_eq!(
            p(i128::MIN, i128::MIN),
            I256 {
                high: 1 << 126,
                low: 0
            }
        );
        assert_eq!(
            p(i128::MIN, i128::MAX),
            I256 {
                high: -(1 << 126),
                low: 1 << 127
            }
        );
        // The order: −2²⁵⁴ + 2¹²⁷ < −1 < 0 < 1 < 2²⁵⁴, and −2 · 3 < −1 · 5.
        let ordered = [
            p(i128::MIN, i128::MAX),
            p(-1, 1),
            p(0, 0),
            p(1, 1),
            p(i128::MIN, i128::MIN),
        ];
        assert!(ordered.is_sorted() && ordered.windows(2).all(|pair| pair[0] != pair[1]));
        assert!(p(-2, 3) < p(-1, 5));
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(10_000))]

        /// Where the product fits `i128`, it orders as `i128`'s does.
        #[test]
        fn products_order_as_narrow_products_do(
            a in any::<i64>(),
            b in any::<i64>(),
            c in any::<i64>(),
            d in any::<i64>(),
        ) {
            let (a, b, c, d) = (i128::from(a), i128::from(b), i128::from(c), i128::from(d));
            let wide = I256::product(a, b).cmp(&I256::product(c, d));
            prop_assert_eq!(wide, (a * b).cmp(&(c * d)));
        }
    }
}
