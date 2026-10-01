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

    /// `self / 2^shift` for a shift from 1 to 127, rounded to nearest, ties to even; `None` when
    /// it passes `u128`.
    pub const fn round_shr(self, shift: u32) -> Option<u128> {
        debug_assert!(0 < shift && shift < 128);
        let floor_high = self.high >> shift;
        if floor_high != 0 {
            return None;
        }
        let floor = (self.high << (128 - shift)) | (self.low >> shift);
        let rest = self.low & ((1 << shift) - 1);
        let half = 1 << (shift - 1);
        if rest > half || (rest == half && floor & 1 == 1) {
            floor.checked_add(1)
        } else {
            Some(floor)
        }
    }

    /// `self / divisor` for a positive divisor below 2¹²⁷, rounded to nearest, ties to even;
    /// `None` when it passes `u128`.
    pub const fn round_div(self, divisor: u128) -> Option<u128> {
        debug_assert!(0 < divisor && divisor < 1 << 127);
        if self.high >= divisor {
            return None;
        }
        // Long division a bit at a time: the remainder stays below the divisor, so below 2¹²⁷,
        // and doubling it never overflows.
        let mut rest = self.high;
        let mut quotient: u128 = 0;
        let mut bit = 128;
        while bit > 0 {
            bit -= 1;
            rest = (rest << 1) | ((self.low >> bit) & 1);
            quotient <<= 1;
            if rest >= divisor {
                rest -= divisor;
                quotient |= 1;
            }
        }
        let twice = rest << 1;
        if twice > divisor || (twice == divisor && quotient & 1 == 1) {
            quotient.checked_add(1)
        } else {
            Some(quotient)
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

        // Halving 5 gives 2.5, ties to the even 2; 7 gives 3.5, to 4; 2¹²⁹ + 2¹ shifted by 2
        // is 2¹²⁷ + 0.5, to the even 2¹²⁷; 2²⁰⁰ shifted by 64 passes u128.
        assert_eq!(U256::product(5, 1).round_shr(1), Some(2));
        assert_eq!(U256::product(7, 1).round_shr(1), Some(4));
        let wide = U256::product(1 << 127, 4).checked_add(U256::product(2, 1));
        assert_eq!(wide.unwrap().round_shr(2), Some(1 << 127));
        assert_eq!(U256::product(1 << 100, 1 << 100).round_shr(64), None);
        assert_eq!(U256::product(u128::MAX, 1).round_shr(1), Some(1 << 127));

        // 7 ÷ 2 = 3.5 ties to 4, 5 ÷ 2 to 2, 10 ÷ 4 = 2.5 to 2, 11 ÷ 4 = 2.75 to 3; (2¹²⁸ + 6) ÷ 3
        // is 2¹²⁸ ÷ 3 rounded, 113427455640312821154458202477256070485.33 to ...485, plus 2;
        // 2¹²⁸ ÷ 1 passes u128.
        assert_eq!(U256::product(7, 1).round_div(2), Some(4));
        assert_eq!(U256::product(5, 1).round_div(2), Some(2));
        assert_eq!(U256::product(10, 1).round_div(4), Some(2));
        assert_eq!(U256::product(11, 1).round_div(4), Some(3));
        let past = U256::product(1 << 64, 1 << 64).checked_add(U256::product(6, 1));
        assert_eq!(
            past.unwrap().round_div(3),
            Some(113_427_455_640_312_821_154_458_202_477_256_070_485 + 2)
        );
        assert_eq!(U256::product(1 << 64, 1 << 64).round_div(1), None);
    }
}
