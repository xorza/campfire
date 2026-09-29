use std::fmt;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::num::decimal::Decimal;
use crate::num::error::ParseNumError;

#[cfg(feature = "bench")]
pub(crate) mod bench;
mod decimal;
pub(crate) mod error;
mod trig;

/// A 40.24 fixed-point number: the value times 2²⁴, in an `i64`.
///
/// `*`, `/`, `sqrt`, `sin_cos` and `atan2` round to nearest, ties to even. Operators panic on
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
        narrow(round_shr(self.0 as i128 * rhs.0 as i128, Self::FRAC_BITS))
    }

    pub const fn checked_div(self, rhs: Num) -> Option<Num> {
        if rhs.0 == 0 {
            return None;
        }
        narrow(round_div(
            (self.0 as i128) << Self::FRAC_BITS,
            rhs.0 as i128,
        ))
    }

    /// Exact scaling by an integer.
    pub const fn checked_mul_int(self, rhs: i64) -> Option<Num> {
        match self.0.checked_mul(rhs) {
            Some(bits) => Some(Num(bits)),
            None => None,
        }
    }

    pub const fn checked_div_int(self, rhs: i64) -> Option<Num> {
        if rhs == 0 {
            return None;
        }
        narrow(round_div(self.0 as i128, rhs as i128))
    }

    /// `None` for a negative value.
    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::float_arithmetic,
        reason = "the f64 root is only an estimate; the integer steps fix the result exactly"
    )]
    pub fn checked_sqrt(self) -> Option<Num> {
        if self.0 < 0 {
            return None;
        }
        let raw = self.0.cast_unsigned();
        let scaled = u128::from(raw) << Self::FRAC_BITS;
        // An f64 estimate is three times as fast as `u128::isqrt`. Below 2⁸⁸ it is within 1 of
        // ⌊√scaled⌋, and the loops make it exact whatever the float returns, so the result
        // stays deterministic.
        // The root stays below 2⁴⁴, so no product or step here can overflow.
        // Converting the 64-bit `raw` is one instruction, where `u128` needs a library call;
        // multiplying by 2²⁴ is exact.
        let mut root = (raw as f64 * f64::from(1_u32 << Self::FRAC_BITS)).sqrt() as u128;
        while root.wrapping_mul(root) > scaled {
            root = root.wrapping_sub(1);
        }
        while (root + 1).wrapping_mul(root + 1) <= scaled {
            root = root.wrapping_add(1);
        }
        // √scaled is never exactly `root + ½`, so rounding up past the midpoint needs no tie rule.
        let root = if scaled.wrapping_sub(root.wrapping_mul(root)) > root {
            root + 1
        } else {
            root
        };
        Some(narrow_in_range(root.cast_signed()))
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
    pub const fn atan2(self, x: Num) -> Num {
        trig::atan2(self, x)
    }
}

/// Narrows an exact result; `None` when it does not fit.
const fn narrow(bits: i128) -> Option<Num> {
    if bits < i64::MIN as i128 || bits > i64::MAX as i128 {
        return None;
    }
    Some(narrow_in_range(bits))
}

/// Narrows a result that cannot overflow.
const fn narrow_in_range(bits: i128) -> Num {
    Num(to_i64(bits))
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "callers pass values that fit i64"
)]
const fn to_i64(value: i128) -> i64 {
    debug_assert!(value >= i64::MIN as i128 && value <= i64::MAX as i128);
    value as i64
}

/// `value / 2^shift`, rounded to nearest, ties to even.
const fn round_shr(value: i128, shift: u32) -> i128 {
    debug_assert!(shift > 0);
    let floor = value >> shift;
    // Neither step can overflow: `rest` is in `[0, 2^shift)` and `floor` is at most `value / 2`.
    let rest = value.wrapping_sub(floor << shift);
    let half = 1 << (shift - 1);
    if rest > half || (rest == half && floor & 1 == 1) {
        floor.wrapping_add(1)
    } else {
        floor
    }
}

/// `numerator / denominator`, rounded to nearest, ties to even. Callers keep the denominator
/// non-zero and below 2¹²⁶, and the quotient at most 2⁸⁷.
const fn round_div(numerator: i128, denominator: i128) -> i128 {
    let n = numerator.unsigned_abs();
    let d = denominator.unsigned_abs();
    debug_assert!(d != 0 && d < 1 << 126);
    let mut magnitude = n / d;
    let rest = n % d;
    // No step can overflow: `rest < d < 2¹²⁶` and the quotient is at most 2⁸⁷.
    let twice_rest = rest << 1;
    if twice_rest > d || (twice_rest == d && magnitude & 1 == 1) {
        magnitude = magnitude.wrapping_add(1);
    }
    debug_assert!(magnitude <= 1 << 87, "round_div quotient above 2⁸⁷");
    let magnitude = magnitude.cast_signed();
    if (numerator < 0) == (denominator < 0) {
        magnitude
    } else {
        magnitude.wrapping_neg()
    }
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
