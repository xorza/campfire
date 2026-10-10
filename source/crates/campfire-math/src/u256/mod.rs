use std::cmp::Ordering;

use crate::rounding::Rounding;

#[cfg(feature = "bench")]
pub(crate) mod bench;

/// An unsigned integer of 256 bits, for exact comparisons of products that pass `u128`: the high
/// half before the low, so the derived order compares numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct U256 {
    high: u128,
    low: u128,
}

/// A division's quotient and its rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Division {
    quotient: u128,
    rest: u128,
}

/// The bits of a half of a `u128`, the digit of products and of long division, and the largest
/// half.
const HALF: u32 = 64;
const MASK: u128 = u64::MAX as u128;

impl U256 {
    pub const ZERO: U256 = U256 { high: 0, low: 0 };

    /// `value`, widened.
    pub const fn from_u128(value: u128) -> U256 {
        U256 {
            high: 0,
            low: value,
        }
    }

    /// The exact product `a × b`, from four products of 64-bit halves.
    pub const fn product(a: u128, b: u128) -> U256 {
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

    /// `self / 2^shift` for a shift from 1 to 127, rounded by `rounding`; `None` when it passes
    /// `u128`.
    pub const fn shr_rounded(self, shift: u32, rounding: Rounding) -> Option<u128> {
        debug_assert!(0 < shift && shift < 128);
        if self.high >> shift != 0 {
            return None;
        }
        let floor = (self.high << (128 - shift)) | (self.low >> shift);
        let rest = self.low & ((1 << shift) - 1);
        U256::rounded(floor, rest, 1 << shift, rounding)
    }

    /// `self / divisor` for a positive divisor, rounded by `rounding`; `None` when it passes
    /// `u128`.
    pub const fn div_rounded(self, divisor: u128, rounding: Rounding) -> Option<u128> {
        debug_assert!(divisor > 0);
        let Some(Division { quotient, rest }) = self.divide(divisor) else {
            return None;
        };
        U256::rounded(quotient, rest, divisor, rounding)
    }

    /// `floor`, or one more where `rounding` takes a rest of `rest` over `divisor` up; `None`
    /// past `u128`.
    const fn rounded(floor: u128, rest: u128, divisor: u128, rounding: Rounding) -> Option<u128> {
        // The rest against what the divisor leaves past it, as twice the rest could pass u128;
        // the rest is below the divisor, so the difference cannot wrap.
        let half = divisor.wrapping_sub(rest);
        if rounding.rounds_up(rest, half, rest == 0, floor & 1 == 1) {
            floor.checked_add(1)
        } else {
            Some(floor)
        }
    }

    /// `self / divisor` rounded down and its rest, `None` when the quotient passes `u128`, as
    /// it does exactly when the high half is at least the divisor. A value that fits `u128`
    /// divides natively.
    const fn divide(self, divisor: u128) -> Option<Division> {
        if self.high >= divisor {
            return None;
        }
        Some(if self.high == 0 {
            Division {
                quotient: self.low / divisor,
                rest: self.low % divisor,
            }
        } else {
            self.long_division(divisor)
        })
    }

    /// `self / divisor` and its rest, for a high half below the divisor, so the quotient fits
    /// `u128`: Knuth's algorithm D on 64-bit digits, as Hacker's Delight's `divlu` gives it, two
    /// quotient digits each from a native division, where a division a bit at a time takes 128
    /// steps. Both sides are shifted until the divisor's top bit is set, which keeps each digit's
    /// estimate from the divisor's top digit at most 2 too large.
    const fn long_division(self, divisor: u128) -> Division {
        debug_assert!(self.high < divisor);
        let shift = divisor.leading_zeros();
        let divisor = divisor << shift;
        // A shift by 128 overflows, so a set top bit leaves the high half as it is.
        let high = if shift == 0 {
            self.high
        } else {
            (self.high << shift) | (self.low >> (128 - shift))
        };
        let low = self.low << shift;
        let upper = U256::digit(high, low >> HALF, divisor);
        let lower = U256::digit(upper.rest, low & MASK, divisor);
        Division {
            quotient: (upper.quotient << HALF) | lower.quotient,
            rest: lower.rest >> shift,
        }
    }

    /// The digit `(high · 2⁶⁴ + next) / divisor` and its rest, for a divisor whose top bit is
    /// set and a `high` below it, so the digit is below 2⁶⁴. The estimate `high / top` is at
    /// least the digit, and each time the divisor's lower digit shows it too large it falls by
    /// one, until its rest by `top` reaches 2⁶⁴, past which the test cannot fail.
    const fn digit(high: u128, next: u128, divisor: u128) -> Division {
        let (top, bottom) = (divisor >> HALF, divisor & MASK);
        let mut quotient = high / top;
        let mut rest = high % top;
        while quotient > MASK || quotient * bottom > ((rest << HALF) | next) {
            quotient -= 1;
            rest += top;
            if rest > MASK {
                break;
            }
        }
        // The true rest lies below the divisor, so the difference taken modulo 2¹²⁸ is that rest.
        let rest = ((high << HALF) | next).wrapping_sub(quotient.wrapping_mul(divisor));
        Division { quotient, rest }
    }

    /// How `self × by` orders against `other × other_by`, exactly: each product, up to 384 bits,
    /// held as three 128-bit limbs, so no product passes them.
    pub const fn cmp_products(self, by: u128, other: U256, other_by: u128) -> Ordering {
        let ours = self.wide_product(by);
        let theirs = other.wide_product(other_by);
        let mut limb = 0;
        while limb < 3 {
            if ours[limb] != theirs[limb] {
                return if ours[limb] < theirs[limb] {
                    Ordering::Less
                } else {
                    Ordering::Greater
                };
            }
            limb += 1;
        }
        Ordering::Equal
    }

    /// `self × by` as three 128-bit limbs, the most significant first. The high half's product
    /// is below 2²⁵⁶ − 2¹²⁸, so its top limb takes the middle limb's carry.
    const fn wide_product(self, by: u128) -> [u128; 3] {
        let low = U256::product(self.low, by);
        let high = U256::product(self.high, by);
        let (middle, carry) = low.high.overflowing_add(high.low);
        [high.high + carry as u128, middle, low.low]
    }

    /// `self × rhs`; `None` past 256 bits.
    pub const fn checked_mul(self, rhs: u128) -> Option<U256> {
        match self.wide_product(rhs) {
            [0, high, low] => Some(U256 { high, low }),
            _ => None,
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
