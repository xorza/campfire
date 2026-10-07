/// An unsigned integer of 256 bits, for exact comparisons of products that pass `u128`: the high
/// half before the low, so the derived order compares numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct U256 {
    high: u128,
    low: u128,
}

impl U256 {
    pub const ZERO: U256 = U256 { high: 0, low: 0 };

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

    /// `self × rhs`; `None` past 256 bits.
    pub const fn checked_mul(self, rhs: u128) -> Option<U256> {
        let low = U256::product(self.low, rhs);
        let high = U256::product(self.high, rhs);
        if high.high != 0 {
            return None;
        }
        let shifted = U256 {
            high: high.low,
            low: 0,
        };
        shifted.checked_add(low)
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

    /// `self − other`; `None` below 0.
    pub const fn checked_sub(self, other: U256) -> Option<U256> {
        let (low, borrow) = self.low.overflowing_sub(other.low);
        let Some(high) = self.high.checked_sub(other.high) else {
            return None;
        };
        match high.checked_sub(borrow as u128) {
            Some(high) => Some(U256 { high, low }),
            None => None,
        }
    }
}

#[cfg(test)]
mod tests;
