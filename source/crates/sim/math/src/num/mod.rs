use std::fmt;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};
use std::str::FromStr;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::num::decimal::Decimal;
use crate::num::error::{IntToNumError, ParseNumError};
use crate::root::RoundedRoot;
use crate::rounding::Rounding;

#[cfg(feature = "internals")]
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
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Num(i64);

/// Sine and cosine of one angle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SinCos {
    pub sin: Num,
    pub cos: Num,
}

impl SinCos {
    /// The sine and the cosine of `angle` degrees, each rounded once from the kernel's value: the
    /// angle reduces in degrees, exactly, so each multiple of 90° is exact, a whole number of
    /// turns more turns alike, and an angle's sine is its complement's cosine.
    pub fn of_degrees(angle: Num) -> Self {
        trig::sin_cos_degrees(angle)
    }
}

impl Num {
    pub const FRAC_BITS: u32 = 24;
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(1 << Self::FRAC_BITS);
    pub const HALF: Self = Self(1 << (Self::FRAC_BITS - 1));
    pub const QUARTER: Self = Self(1 << (Self::FRAC_BITS - 2));
    /// The smallest positive value, 2⁻²⁴.
    pub const EPSILON: Self = Self(1);
    pub const MIN: Self = Self(i64::MIN);
    pub const MAX: Self = Self(i64::MAX);
    pub const PI: Self = Self(trig::pi_bits(Self::FRAC_BITS));
    pub const TAU: Self = Self(trig::pi_bits(Self::FRAC_BITS + 1));
    pub const FRAC_PI_2: Self = Self(trig::pi_bits(Self::FRAC_BITS - 1));

    const HALF_BITS: i64 = 1 << (Self::FRAC_BITS - 1);
    const FRAC_MASK: i64 = (1 << Self::FRAC_BITS) - 1;

    pub const fn from_bits(bits: i64) -> Self {
        Self(bits)
    }

    pub const fn to_bits(self) -> i64 {
        self.0
    }

    /// Exact conversion; `None` outside `−2³⁹ ≤ value < 2³⁹`.
    pub const fn from_int(value: i64) -> Option<Self> {
        match value.checked_mul(Self::ONE.0) {
            Some(bits) => Some(Self(bits)),
            None => None,
        }
    }

    /// `value`, which must lie in `−2³⁹ ≤ value < 2³⁹`, as a constant or a literal does; for a
    /// value from data, `from_int`.
    pub const fn int(value: i64) -> Self {
        match Self::from_int(value) {
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

    pub const fn checked_add(self, rhs: Self) -> Option<Self> {
        match self.0.checked_add(rhs.0) {
            Some(bits) => Some(Self(bits)),
            None => None,
        }
    }

    pub const fn checked_sub(self, rhs: Self) -> Option<Self> {
        match self.0.checked_sub(rhs.0) {
            Some(bits) => Some(Self(bits)),
            None => None,
        }
    }

    pub const fn checked_neg(self) -> Option<Self> {
        match self.0.checked_neg() {
            Some(bits) => Some(Self(bits)),
            None => None,
        }
    }

    /// The number of `bits`, at the end of the range of numbers when past it.
    pub const fn saturating_from_bits(bits: i128) -> Self {
        if bits < i64::MIN as i128 {
            Self::MIN
        } else if bits > i64::MAX as i128 {
            Self::MAX
        } else {
            Self::from_wide_bits(bits)
        }
    }

    /// `self × rhs`, from the exact product, rounded once by `rounding`, and at the end of the
    /// range of numbers when past it: a step of a compound value, as a stat's
    /// (07-determinism-core.md, A compound value is a chain of rounded steps).
    #[must_use]
    pub const fn saturating_mul(self, rhs: Self, rounding: Rounding) -> Self {
        let product = (self.0 as i128).wrapping_mul(rhs.0 as i128);
        Self::saturating_from_bits(rounding.shift_right(product, Self::FRAC_BITS))
    }

    /// `self × rhs`, from the exact product, rounded once by `rounding`; `None` for a result that
    /// does not fit.
    pub const fn checked_mul(self, rhs: Self, rounding: Rounding) -> Option<Self> {
        narrow(rounding.shift_right(self.0 as i128 * rhs.0 as i128, Self::FRAC_BITS))
    }

    /// `self × by × and`, from the exact product of the three, rounded once by `rounding`; `None`
    /// for a result that does not fit. A product past an `i128` is far past every `Num`.
    pub const fn checked_mul_mul(self, by: Self, and: Self, rounding: Rounding) -> Option<Self> {
        let Some(product) = (self.0 as i128 * by.0 as i128).checked_mul(and.0 as i128) else {
            return None;
        };
        narrow(rounding.shift_right(product, 2 * Self::FRAC_BITS))
    }

    /// The value with these bits, from a wider result that cannot overflow.
    pub(crate) const fn from_wide_bits(bits: i128) -> Self {
        Self(to_i64(bits))
    }

    /// The value nearest to `sum / 2²⁴`, for a sum of products of raw values, rounded once to
    /// nearest, ties to even: a wide sum's one rounding. `None` when it does not fit.
    pub(crate) const fn from_raw_products(sum: i128) -> Option<Self> {
        narrow(Rounding::NearestEven.shift_right(sum, Self::FRAC_BITS))
    }

    /// `self ÷ rhs`, rounded once by `rounding`; `None` for a divisor of 0 or a result that does
    /// not fit.
    pub fn checked_div(self, rhs: Self, rounding: Rounding) -> Option<Self> {
        if rhs.0 == 0 {
            return None;
        }
        narrow(rounding.divide(i128::from(self.0) << Self::FRAC_BITS, i128::from(rhs.0)))
    }

    /// `self × by ÷ over`, from the exact product, rounded once by `rounding`; `None` for a
    /// divisor of 0 or a result that does not fit.
    pub fn checked_mul_div(self, by: Self, over: Self, rounding: Rounding) -> Option<Self> {
        if over.0 == 0 {
            return None;
        }
        narrow(rounding.divide(i128::from(self.0) * i128::from(by.0), i128::from(over.0)))
    }

    /// `self × by ÷ over` for whole `by` and `over`, as a share of a value, from the exact
    /// product, rounded once by `rounding`; `None` for a divisor of 0 or a result that does not
    /// fit.
    pub fn checked_mul_div_int(self, by: i64, over: i64, rounding: Rounding) -> Option<Self> {
        if over == 0 {
            return None;
        }
        narrow(rounding.divide(i128::from(self.0) * i128::from(by), i128::from(over)))
    }

    /// `count` whole units times `self`, from the exact product, rounded once by `rounding` to a
    /// whole count, as a share of a span of ticks takes; `None` for a product below 0, of a
    /// `self` below 0, or past `u64`. The product of any `Num` and any `u64` fits an `i128`.
    pub fn scale_count(self, count: u64, rounding: Rounding) -> Option<u64> {
        let product = i128::from(self.0) * i128::from(count);
        u64::try_from(rounding.shift_right(product, Self::FRAC_BITS)).ok()
    }

    /// Exact scaling by an integer.
    pub const fn checked_mul_int(self, rhs: i64) -> Option<Self> {
        match self.0.checked_mul(rhs) {
            Some(bits) => Some(Self(bits)),
            None => None,
        }
    }

    /// `self ÷ rhs` for a whole `rhs`, rounded once by `rounding`; `None` for a divisor of 0 or
    /// a result that does not fit.
    pub fn checked_div_int(self, rhs: i64, rounding: Rounding) -> Option<Self> {
        if rhs == 0 {
            return None;
        }
        narrow(rounding.divide(i128::from(self.0), i128::from(rhs)))
    }

    /// `None` for a negative value.
    pub fn checked_sqrt(self) -> Option<Self> {
        if self.0 < 0 {
            return None;
        }
        Self::from_root_of_bits(u128::from(self.0.cast_unsigned()) << Self::FRAC_BITS)
    }

    /// The value whose bits are the integer nearest to √`squared_bits`; `None` when it does not
    /// fit. √ of an integer is never exactly a half, so no tie rule is needed.
    pub(crate) fn from_root_of_bits(squared_bits: u128) -> Option<Self> {
        // From 2¹²⁶ on, the root is at least 2⁶³, past `i64`.
        if squared_bits >= 1 << 126 {
            return None;
        }
        narrow(squared_bits.nearest_root().cast_signed())
    }

    #[must_use]
    pub fn sqrt(self) -> Self {
        self.checked_sqrt()
            .expect("Num square root of a negative value")
    }

    /// Sine and cosine of an angle in radians; any angle is reduced exactly.
    pub const fn sin_cos(self) -> SinCos {
        trig::sin_cos(self)
    }

    /// The angle of the point `(x, self)` in radians, in `[−π, π]`; `0` for the origin.
    #[must_use]
    pub fn atan2(self, x: Self) -> Self {
        trig::atan2(self, x)
    }

    /// The angle of the point `(x, self)` in degrees, in `[−180, 180]`, rounded once from the
    /// angle `atan2` finds before it rounds; `0` for the origin.
    #[must_use]
    pub fn atan2_degrees(self, x: Self) -> Self {
        trig::atan2_degrees(self, x)
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
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        self.checked_add(rhs).expect("Num overflow in +")
    }
}

impl Sub for Num {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        self.checked_sub(rhs).expect("Num overflow in -")
    }
}

impl Neg for Num {
    type Output = Self;

    fn neg(self) -> Self {
        self.checked_neg().expect("Num overflow in unary -")
    }
}

impl Mul for Num {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self {
        self.checked_mul(rhs, Rounding::NearestEven)
            .expect("Num overflow in *")
    }
}

impl Div for Num {
    type Output = Self;

    fn div(self, rhs: Self) -> Self {
        self.checked_div(rhs, Rounding::NearestEven)
            .expect("Num division by zero or overflow in /")
    }
}

impl Mul<i64> for Num {
    type Output = Self;

    fn mul(self, rhs: i64) -> Self {
        self.checked_mul_int(rhs).expect("Num overflow in * i64")
    }
}

impl Div<i64> for Num {
    type Output = Self;

    fn div(self, rhs: i64) -> Self {
        self.checked_div_int(rhs, Rounding::NearestEven)
            .expect("Num division by zero or overflow in / i64")
    }
}

impl AddAssign for Num {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl SubAssign for Num {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl MulAssign for Num {
    fn mul_assign(&mut self, rhs: Self) {
        *self = *self * rhs;
    }
}

impl DivAssign for Num {
    fn div_assign(&mut self, rhs: Self) {
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

/// Exact: every `i8` lies within the integers a `Num` holds.
impl From<i8> for Num {
    fn from(value: i8) -> Self {
        Self::int(i64::from(value))
    }
}

/// Exact: every `u8` lies within the integers a `Num` holds.
impl From<u8> for Num {
    fn from(value: u8) -> Self {
        Self::int(i64::from(value))
    }
}

/// Exact: every `i16` lies within the integers a `Num` holds.
impl From<i16> for Num {
    fn from(value: i16) -> Self {
        Self::int(i64::from(value))
    }
}

/// Exact: every `u16` lies within the integers a `Num` holds.
impl From<u16> for Num {
    fn from(value: u16) -> Self {
        Self::int(i64::from(value))
    }
}

/// Exact: every `i32` lies within the integers a `Num` holds.
impl From<i32> for Num {
    fn from(value: i32) -> Self {
        Self::int(i64::from(value))
    }
}

/// Exact: every `u32` lies within the integers a `Num` holds.
impl From<u32> for Num {
    fn from(value: u32) -> Self {
        Self::int(i64::from(value))
    }
}

impl TryFrom<i64> for Num {
    type Error = IntToNumError;

    fn try_from(value: i64) -> Result<Self, IntToNumError> {
        Self::from_int(value).ok_or(IntToNumError::OutOfRange)
    }
}

impl TryFrom<u64> for Num {
    type Error = IntToNumError;

    fn try_from(value: u64) -> Result<Self, IntToNumError> {
        let value = i64::try_from(value).ok();
        value
            .and_then(Self::from_int)
            .ok_or(IntToNumError::OutOfRange)
    }
}

impl FromStr for Num {
    type Err = ParseNumError;

    /// Reads `[-]digits[.digits]` exactly, rounded to nearest, ties to even.
    fn from_str(text: &str) -> Result<Self, ParseNumError> {
        decimal::parse(text.as_bytes())
    }
}

impl fmt::Display for Num {
    /// The exact decimal value; 24 fractional bits always end within 24 decimal digits.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(Decimal::new(*self).as_str())
    }
}

/// In a binary format, which the log, the state hash and the network read, its bits; in a text
/// format, which data files read, its exact decimal string.
impl Serialize for Num {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            serializer.collect_str(self)
        } else {
            serializer.serialize_i64(self.0)
        }
    }
}

/// In a binary format, its bits; in a text format, an integer or a decimal string, as data writes
/// a number, where an integer past what a `Num` holds, a float, and text `from_str` refuses fail
/// the read, as data never holds a float.
impl<'de> Deserialize<'de> for Num {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if deserializer.is_human_readable() {
            deserializer.deserialize_any(TextNum)
        } else {
            i64::deserialize(deserializer).map(Num)
        }
    }
}

/// The reader of a `Num` from a text format.
#[derive(Debug)]
struct TextNum;

impl Visitor<'_> for TextNum {
    type Value = Num;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an integer or a decimal string")
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Num, E> {
        Num::from_int(value)
            .ok_or_else(|| E::custom(format!("{value} is past what a number holds")))
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Num, E> {
        Num::try_from(value).map_err(|_| E::custom(format!("{value} is past what a number holds")))
    }

    fn visit_str<E: de::Error>(self, text: &str) -> Result<Num, E> {
        Num::from_str(text).map_err(E::custom)
    }
}

#[cfg(test)]
mod tests;
