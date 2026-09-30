/// An unsigned integer of 256 bits, for exact comparisons of products that pass `u128`: the high
/// half before the low, so the derived order compares numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct U256 {
    high: u128,
    low: u128,
}

impl U256 {
    /// The exact product `a × b`, from four products of 64-bit halves.
    pub const fn product(a: u128, b: u128) -> U256 {
        const HALF: u32 = 64;
        const MASK: u128 = u64::MAX as u128;
        let (a_low, a_high) = (a & MASK, a >> HALF);
        let (b_low, b_high) = (b & MASK, b >> HALF);
        let low = a_low * b_low;
        let middle_a = a_high * b_low;
        let middle_b = a_low * b_high;
        let high = a_high * b_high;
        // The middle terms sit 64 bits up; their sum with the low term's carry fits 130 bits,
        // so it is added in two steps.
        let middle = (middle_a & MASK) + (middle_b & MASK) + (low >> HALF);
        U256 {
            high: high + (middle_a >> HALF) + (middle_b >> HALF) + (middle >> HALF),
            low: (middle << HALF) | (low & MASK),
        }
    }

    /// `self + other`; `None` past 256 bits.
    pub const fn checked_add(self, other: U256) -> Option<U256> {
        let (low, carry) = self.low.overflowing_add(other.low);
        let Some(high) = self.high.checked_add(other.high) else {
            return None;
        };
        match high.checked_add(carry as u128) {
            Some(high) => Some(U256 { high, low }),
            None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn products_and_sums_are_exact() {
        // Within u128 the high half is 0.
        assert_eq!(U256::product(6, 7), U256 { high: 0, low: 42 });
        // 2¹²⁷ × 4 = 2¹²⁹: bit 1 of the high half.
        assert_eq!(U256::product(1 << 127, 4), U256 { high: 2, low: 0 });
        // (2¹²⁸ − 1)² = 2²⁵⁶ − 2¹²⁹ + 1: high 2¹²⁸ − 2, low 1.
        let max = U256::product(u128::MAX, u128::MAX);
        assert_eq!(
            max,
            U256 {
                high: u128::MAX - 1,
                low: 1
            }
        );
        // (2⁶⁴ + 3)(2⁶⁴ + 5) = 2¹²⁸ + 8 × 2⁶⁴ + 15: every cross term carries into place.
        let cross = U256::product((1 << 64) + 3, (1 << 64) + 5);
        assert_eq!(
            cross,
            U256 {
                high: 1,
                low: (8 << 64) + 15
            }
        );

        // A carry from the low half; and past 256 bits, none.
        let one_short = U256 {
            high: 0,
            low: u128::MAX,
        };
        let one = U256 { high: 0, low: 1 };
        assert_eq!(one_short.checked_add(one), Some(U256 { high: 1, low: 0 }));
        let top = U256 {
            high: u128::MAX,
            low: u128::MAX,
        };
        assert_eq!(top.checked_add(one), None);

        // The order compares the high half first.
        assert!(
            U256 { high: 1, low: 0 }
                > U256 {
                    high: 0,
                    low: u128::MAX
                }
        );
        assert!(U256::product(3, 5) < U256::product(4, 4));
    }
}
