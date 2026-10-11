use std::fmt;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::floor_root::NearestRoot;
use crate::num::decimal::Decimal;
use crate::num::error::ParseNumError;
use crate::rounding::Rounding;

#[cfg(feature = "bench")]
pub(crate) mod bench;
mod decimal;
pub(crate) mod error;
mod trig;

/// A 40.24 fixed-point number: the value times 2²⁴, in an `i64`.
///
/// `*`, `/` and `sqrt` round to nearest, ties to even. `sin_cos` and `atan2` are within 0.501
/// ulp of the exact value over the tests' sweeps, and give the same bits on every machine, but
/// no search of the hard cases proves them correctly rounded. Operators panic on
/// overflow and on division by zero, which in engine code are bugs; the `checked_*` methods
/// return `None` instead.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
#[repr(transparent)]
pub struct Num(i64);

/// Sine and cosine of one angle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SinCos {
    pub sin: Num,
    pub cos: Num,
}

impl Num {
    pub const FRAC_BITS: u32 = 24;
    pub const ZERO: Num = Num(0);
    pub const ONE: Num = Num(1 << Self::FRAC_BITS);
    pub const HALF: Num = Num(1 << (Self::FRAC_BITS - 1));
    pub const QUARTER: Num = Num(1 << (Self::FRAC_BITS - 2));
    /// The smallest positive value, 2⁻²⁴.
    pub const EPSILON: Num = Num(1);
    pub const MIN: Num = Num(i64::MIN);
    pub const MAX: Num = Num(i64::MAX);
    pub const PI: Num = Num(trig::pi_bits(Self::FRAC_BITS));
    pub const TAU: Num = Num(trig::pi_bits(Self::FRAC_BITS + 1));
    pub const FRAC_PI_2: Num = Num(trig::pi_bits(Self::FRAC_BITS - 1));

    const HALF_BITS: i64 = 1 << (Self::FRAC_BITS - 1);
    const FRAC_MASK: i64 = (1 << Self::FRAC_BITS) - 1;

    pub const fn from_bits(bits: i64) -> Num {
        Num(bits)
    }

    pub const fn to_bits(self) -> i64 {
        self.0
    }

    /// Exact conversion; `None` outside `−2³⁹ ≤ value < 2³⁹`.
    pub const fn from_int(value: i64) -> Option<Num> {
        match value.checked_mul(Self::ONE.0) {
            Some(bits) => Some(Num(bits)),
            None => None,
        }
    }

    /// `value`, which must lie in `−2³⁹ ≤ value < 2³⁹`, as a constant or a literal does; for a
    /// value from data, `from_int`.
    pub const fn int(value: i64) -> Num {
        match Num::from_int(value) {
            Some(num) => num,
            None => panic!("an integer within a number's range"),
        }
    }

    /// Exact conversion; `None` for a value with a fraction.
    pub const fn to_int(self) -> Option<i64> {
        if self.0 & Self::FRAC_MASK == 0 {
            Some(self.floor())
        } else {
            None
        }
    }

    pub const fn floor(self) -> i64 {
        self.0 >> Self::FRAC_BITS
    }

    pub const fn ceil(self) -> i64 {
        if self.0 & Self::FRAC_MASK == 0 {
            self.floor()
        } else {
            self.floor() + 1
        }
    }

    /// Rounds to the nearest integer, halves away from zero: `2.5 → 3`, `−2.5 → −3`.
    pub const fn round(self) -> i64 {
        let frac = self.0 & Self::FRAC_MASK;
        let up = if self.0 >= 0 {
            frac >= Self::HALF_BITS
        } else {
            frac > Self::HALF_BITS
        };
        if up { self.floor() + 1 } else { self.floor() }
    }

    pub const fn checked_add(self, rhs: Num) -> Option<Num> {
        match self.0.checked_add(rhs.0) {
            Some(bits) => Some(Num(bits)),
            None => None,
        }
    }

    pub const fn checked_sub(self, rhs: Num) -> Option<Num> {
        match self.0.checked_sub(rhs.0) {
            Some(bits) => Some(Num(bits)),
            None => None,
        }
    }

    pub const fn checked_neg(self) -> Option<Num> {
        match self.0.checked_neg() {
            Some(bits) => Some(Num(bits)),
            None => None,
        }
    }

    pub const fn checked_mul(self, rhs: Num) -> Option<Num> {
        Num::from_raw_products(self.0 as i128 * rhs.0 as i128)
    }

    /// The value with these bits, from a wider result that cannot overflow.
    pub(crate) const fn from_wide_bits(bits: i128) -> Num {
        Num(to_i64(bits))
    }

    /// The value nearest to `sum / 2²⁴`, for a sum of products of raw values, rounded once to
    /// nearest, ties to even: a wide sum's one rounding. `None` when it does not fit. Adding
    /// `2²³ − 1` and the floor's last bit before the shift rounds a rest past half up, a rest of
    /// half up from an odd floor only, and a smaller rest down. The addition wraps only for a sum
    /// within 2²⁴ of `i128::MAX`, whose quotient, wrapped or not, does not fit an `i64` either.
    pub(crate) const fn from_raw_products(sum: i128) -> Option<Num> {
        const HALF_LESS_ONE: i128 = (1 << (Num::FRAC_BITS - 1)) - 1;
        let odd = (sum >> Self::FRAC_BITS) & 1;
        narrow(sum.wrapping_add(HALF_LESS_ONE + odd) >> Self::FRAC_BITS)
    }

    /// The value whose bits are nearest to `numerator / denominator`, for a ratio of raw products;
    /// `None` when it does not fit.
    pub(crate) fn from_raw_ratio(numerator: i128, denominator: i128) -> Option<Num> {
        narrow(Rounding::NearestEven.divide(numerator, denominator))
    }

    pub fn checked_div(self, rhs: Num) -> Option<Num> {
        self.checked_div_rounded(rhs, Rounding::NearestEven)
    }

    /// `self ÷ rhs`, rounded once by `rounding`; `None` for a divisor of 0 or a result that does
    /// not fit.
    pub fn checked_div_rounded(self, rhs: Num, rounding: Rounding) -> Option<Num> {
        if rhs.0 == 0 {
            return None;
        }
        narrow(rounding.divide(i128::from(self.0) << Self::FRAC_BITS, i128::from(rhs.0)))
    }

    /// `self × by ÷ over`, from the exact product, rounded once by `rounding`; `None` for a
    /// divisor of 0 or a result that does not fit.
    pub fn checked_mul_div(self, by: Num, over: Num, rounding: Rounding) -> Option<Num> {
        if over.0 == 0 {
            return None;
        }
        narrow(rounding.divide(i128::from(self.0) * i128::from(by.0), i128::from(over.0)))
    }

    /// `self × by ÷ over` for whole `by` and `over`, as a share of a value, from the exact
    /// product, rounded once by `rounding`; `None` for a divisor of 0 or a result that does not
    /// fit.
    pub fn checked_mul_ratio(self, by: i64, over: i64, rounding: Rounding) -> Option<Num> {
        if over == 0 {
            return None;
        }
        narrow(rounding.divide(i128::from(self.0) * i128::from(by), i128::from(over)))
    }

    /// Exact scaling by an integer.
    pub const fn checked_mul_int(self, rhs: i64) -> Option<Num> {
        match self.0.checked_mul(rhs) {
            Some(bits) => Some(Num(bits)),
            None => None,
        }
    }

    /// `self × rhs ÷ divisor`, rounded once to nearest, ties to even; `None` for a divisor of 0
    /// or a result that does not fit.
    pub fn checked_mul_div_int(self, rhs: Num, divisor: i64) -> Option<Num> {
        if divisor == 0 {
            return None;
        }
        narrow(Rounding::NearestEven.divide(
            i128::from(self.0) * i128::from(rhs.0),
            i128::from(divisor) << Self::FRAC_BITS,
        ))
    }

    pub fn checked_div_int(self, rhs: i64) -> Option<Num> {
        if rhs == 0 {
            return None;
        }
        narrow(Rounding::NearestEven.divide(i128::from(self.0), i128::from(rhs)))
    }

    /// `None` for a negative value.
    pub fn checked_sqrt(self) -> Option<Num> {
        if self.0 < 0 {
            return None;
        }
        Num::from_root_of_bits(u128::from(self.0.cast_unsigned()) << Self::FRAC_BITS)
    }

    /// The value whose bits are the integer nearest to √`squared_bits`; `None` when it does not
    /// fit. √ of an integer is never exactly a half, so no tie rule is needed.
    pub(crate) fn from_root_of_bits(squared_bits: u128) -> Option<Num> {
        // From 2¹²⁶ on, the root is at least 2⁶³, past `i64`.
        if squared_bits >= 1 << 126 {
            return None;
        }
        narrow(squared_bits.nearest_root().cast_signed())
    }

    #[must_use]
    pub fn sqrt(self) -> Num {
        self.checked_sqrt()
            .expect("Num square root of a negative value")
    }

    /// Sine and cosine of an angle in radians; any angle is reduced exactly.
    pub const fn sin_cos(self) -> SinCos {
        trig::sin_cos(self)
    }

    /// The angle of the point `(x, self)` in radians, in `[−π, π]`; `0` for the origin.
    #[must_use]
    pub fn atan2(self, x: Num) -> Num {
        trig::atan2(self, x)
    }
}

/// Narrows an exact result; `None` when it does not fit.
const fn narrow(bits: i128) -> Option<Num> {
    if bits < i64::MIN as i128 || bits > i64::MAX as i128 {
        return None;
    }
    Some(Num::from_wide_bits(bits))
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "callers pass values that fit i64"
)]
const fn to_i64(value: i128) -> i64 {
    debug_assert!(value >= i64::MIN as i128 && value <= i64::MAX as i128);
    value as i64
}

impl Add for Num {
    type Output = Num;

    fn add(self, rhs: Num) -> Num {
        self.checked_add(rhs).expect("Num overflow in +")
    }
}

impl Sub for Num {
    type Output = Num;

    fn sub(self, rhs: Num) -> Num {
        self.checked_sub(rhs).expect("Num overflow in -")
    }
}

impl Neg for Num {
    type Output = Num;

    fn neg(self) -> Num {
        self.checked_neg().expect("Num overflow in unary -")
    }
}

impl Mul for Num {
    type Output = Num;

    fn mul(self, rhs: Num) -> Num {
        self.checked_mul(rhs).expect("Num overflow in *")
    }
}

impl Div for Num {
    type Output = Num;

    fn div(self, rhs: Num) -> Num {
        self.checked_div(rhs)
            .expect("Num division by zero or overflow in /")
    }
}

impl Mul<i64> for Num {
    type Output = Num;

    fn mul(self, rhs: i64) -> Num {
        self.checked_mul_int(rhs).expect("Num overflow in * i64")
    }
}

impl Div<i64> for Num {
    type Output = Num;

    fn div(self, rhs: i64) -> Num {
        self.checked_div_int(rhs)
            .expect("Num division by zero or overflow in / i64")
    }
}

impl AddAssign for Num {
    fn add_assign(&mut self, rhs: Num) {
        *self = *self + rhs;
    }
}

impl SubAssign for Num {
    fn sub_assign(&mut self, rhs: Num) {
        *self = *self - rhs;
    }
}

impl MulAssign for Num {
    fn mul_assign(&mut self, rhs: Num) {
        *self = *self * rhs;
    }
}

impl DivAssign for Num {
    fn div_assign(&mut self, rhs: Num) {
        *self = *self / rhs;
    }
}

impl MulAssign<i64> for Num {
    fn mul_assign(&mut self, rhs: i64) {
        *self = *self * rhs;
    }
}

impl DivAssign<i64> for Num {
    fn div_assign(&mut self, rhs: i64) {
        *self = *self / rhs;
    }
}

impl FromStr for Num {
    type Err = ParseNumError;

    /// Reads `[-]digits[.digits]` exactly, rounded to nearest, ties to even.
    fn from_str(text: &str) -> Result<Num, ParseNumError> {
        decimal::parse(text.as_bytes())
    }
}

impl fmt::Display for Num {
    /// The exact decimal value; 24 fractional bits always end within 24 decimal digits.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(Decimal::new(*self).as_str())
    }
}

#[cfg(test)]
mod tests;
