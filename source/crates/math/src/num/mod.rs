use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

use serde::{Deserialize, Serialize};

/// A 40.24 fixed-point number: the value times 2²⁴, in an `i64`.
///
/// `*` and `/` round to nearest, ties to even. Operators panic on overflow and on division by
/// zero, which in engine code are bugs; the `checked_*` methods return `None` instead.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
#[repr(transparent)]
pub struct Num(i64);

impl Num {
    pub const FRAC_BITS: u32 = 24;
    pub const ZERO: Num = Num(0);
    pub const ONE: Num = Num(1 << Self::FRAC_BITS);
    /// The smallest positive value, 2⁻²⁴.
    pub const EPSILON: Num = Num(1);
    pub const MIN: Num = Num(i64::MIN);
    pub const MAX: Num = Num(i64::MAX);

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

    pub fn checked_add(self, rhs: Num) -> Option<Num> {
        self.0.checked_add(rhs.0).map(Num)
    }

    pub fn checked_sub(self, rhs: Num) -> Option<Num> {
        self.0.checked_sub(rhs.0).map(Num)
    }

    pub fn checked_neg(self) -> Option<Num> {
        self.0.checked_neg().map(Num)
    }

    pub fn checked_mul(self, rhs: Num) -> Option<Num> {
        let product = i128::from(self.0) * i128::from(rhs.0);
        i64::try_from(div_one_rounded(product)).ok().map(Num)
    }

    pub fn checked_div(self, rhs: Num) -> Option<Num> {
        if rhs.0 == 0 {
            return None;
        }
        let quotient = round_div(i128::from(self.0) << Self::FRAC_BITS, i128::from(rhs.0));
        i64::try_from(quotient).ok().map(Num)
    }

    /// Exact scaling by an integer.
    pub fn checked_mul_int(self, rhs: i64) -> Option<Num> {
        self.0.checked_mul(rhs).map(Num)
    }

    pub fn checked_div_int(self, rhs: i64) -> Option<Num> {
        if rhs == 0 {
            return None;
        }
        i64::try_from(round_div(i128::from(self.0), i128::from(rhs)))
            .ok()
            .map(Num)
    }
}

/// `value / 2²⁴`, rounded to nearest, ties to even.
fn div_one_rounded(value: i128) -> i128 {
    let floor = value >> Num::FRAC_BITS;
    let rest = value - (floor << Num::FRAC_BITS);
    let half = i128::from(Num::HALF_BITS);
    if rest > half || (rest == half && floor & 1 == 1) {
        floor + 1
    } else {
        floor
    }
}

/// `numerator / denominator`, rounded to nearest, ties to even. The denominator is not zero, and
/// callers keep `|numerator|` at most 2⁸⁷.
fn round_div(numerator: i128, denominator: i128) -> i128 {
    let n = numerator.unsigned_abs();
    let d = denominator.unsigned_abs();
    let mut magnitude = n / d;
    let rest = n % d;
    if 2 * rest > d || (2 * rest == d && magnitude & 1 == 1) {
        magnitude += 1;
    }
    debug_assert!(magnitude <= 1 << 87, "round_div numerator above 2⁸⁷");
    #[expect(
        clippy::cast_possible_wrap,
        reason = "magnitude ≤ 2⁸⁷, far inside i128"
    )]
    let magnitude = magnitude as i128;
    if (numerator < 0) == (denominator < 0) {
        magnitude
    } else {
        -magnitude
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

#[cfg(test)]
mod tests;
