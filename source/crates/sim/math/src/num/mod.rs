use std::fmt;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};
use std::str::FromStr;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::num::decimal::Decimal;
use crate::num::error::{FloatToNumError, IntToNumError, ParseNumError};
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

    /// The number of `bits`, at the end of the range of numbers when past it.
    pub const fn saturating_from_bits(bits: i128) -> Num {
        if bits < i64::MIN as i128 {
            Num::MIN
        } else if bits > i64::MAX as i128 {
            Num::MAX
        } else {
            Num::from_wide_bits(bits)
        }
    }

    /// `self × rhs`, rounded once to nearest, ties to even, and at the end of the range of
    /// numbers when past it: a step of a compound value, as a stat's (design 09, D11).
    #[must_use]
    pub const fn saturating_mul(self, rhs: Num) -> Num {
        let product = (self.0 as i128).wrapping_mul(rhs.0 as i128);
        Num::saturating_from_bits(Rounding::NearestEven.shift_right(product, Num::FRAC_BITS))
    }

    pub const fn checked_mul(self, rhs: Num) -> Option<Num> {
        Num::from_raw_products(self.0 as i128 * rhs.0 as i128)
    }

    /// The value with these bits, from a wider result that cannot overflow.
    pub(crate) const fn from_wide_bits(bits: i128) -> Num {
        Num(to_i64(bits))
    }

    /// The value nearest to `sum / 2²⁴`, for a sum of products of raw values, rounded once to
    /// nearest, ties to even: a wide sum's one rounding. `None` when it does not fit.
    pub(crate) const fn from_raw_products(sum: i128) -> Option<Num> {
        narrow(Rounding::NearestEven.shift_right(sum, Num::FRAC_BITS))
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

    /// `count` whole units times `self`, from the exact product, rounded once by `rounding` to a
    /// whole count, as a share of a span of ticks takes; `None` for a count below 0 or past
    /// `u64`. The product of any `Num` and any `u64` fits an `i128`.
    pub fn scale_count(self, count: u64, rounding: Rounding) -> Option<u64> {
        let product = i128::from(self.0) * i128::from(count);
        u64::try_from(rounding.shift_right(product, Self::FRAC_BITS)).ok()
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

    /// The angle of the point `(x, self)` in degrees, in `[−180, 180]`, rounded once from the
    /// angle `atan2` finds before it rounds; `0` for the origin.
    #[must_use]
    pub fn atan2_degrees(self, x: Num) -> Num {
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

/// Exact: every `i8` lies within the integers a `Num` holds.
impl From<i8> for Num {
    fn from(value: i8) -> Num {
        Num::int(i64::from(value))
    }
}

/// Exact: every `u8` lies within the integers a `Num` holds.
impl From<u8> for Num {
    fn from(value: u8) -> Num {
        Num::int(i64::from(value))
    }
}

/// Exact: every `i16` lies within the integers a `Num` holds.
impl From<i16> for Num {
    fn from(value: i16) -> Num {
        Num::int(i64::from(value))
    }
}

/// Exact: every `u16` lies within the integers a `Num` holds.
impl From<u16> for Num {
    fn from(value: u16) -> Num {
        Num::int(i64::from(value))
    }
}

/// Exact: every `i32` lies within the integers a `Num` holds.
impl From<i32> for Num {
    fn from(value: i32) -> Num {
        Num::int(i64::from(value))
    }
}

/// Exact: every `u32` lies within the integers a `Num` holds.
impl From<u32> for Num {
    fn from(value: u32) -> Num {
        Num::int(i64::from(value))
    }
}

impl TryFrom<i64> for Num {
    type Error = IntToNumError;

    fn try_from(value: i64) -> Result<Num, IntToNumError> {
        Num::from_int(value).ok_or(IntToNumError::OutOfRange)
    }
}

impl TryFrom<u64> for Num {
    type Error = IntToNumError;

    fn try_from(value: u64) -> Result<Num, IntToNumError> {
        let value = i64::try_from(value).ok();
        value
            .and_then(Num::from_int)
            .ok_or(IntToNumError::OutOfRange)
    }
}

/// `value` exactly where a step of 2⁻²⁴ holds it, as it holds every value of at least 2⁻¹ that
/// fits, and below rounded once to nearest, ties to even.
impl TryFrom<f32> for Num {
    type Error = FloatToNumError;

    fn try_from(value: f32) -> Result<Num, FloatToNumError> {
        if !value.is_finite() {
            return Err(FloatToNumError::NotFinite);
        }
        let bits = value.to_bits();
        let exponent = i32::try_from((bits >> 23) & 0xFF).expect("eight bits");
        let fraction = i128::from(bits & 0x7F_FFFF);
        let (mantissa, power) = if exponent == 0 {
            (fraction, -149)
        } else {
            (fraction | 0x80_0000, exponent - 150)
        };
        // The value is `mantissa · 2^power`, so its bits are `mantissa · 2^(power + 24)`: a left
        // shift below 64 that fits an `i64` or none, or a right shift of at most 125.
        let scaled = power + Num::FRAC_BITS.cast_signed();
        let magnitude = if scaled >= 0 {
            if scaled >= 64 {
                return Err(FloatToNumError::OutOfRange);
            }
            mantissa << scaled
        } else {
            Rounding::NearestEven.shift_right(mantissa, scaled.unsigned_abs())
        };
        let bits = if value.is_sign_negative() {
            -magnitude
        } else {
            magnitude
        };
        let bits = i64::try_from(bits).map_err(|_| FloatToNumError::OutOfRange)?;
        Ok(Num::from_bits(bits))
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
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Num, D::Error> {
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
