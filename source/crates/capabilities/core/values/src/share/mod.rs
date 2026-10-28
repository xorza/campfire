use std::str::FromStr;

use campfire_math::{Num, Rounding};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::ShareError;

/// A share from 0 to 1, exactly as data writes it: the integer 0 or 1, or a decimal string of at
/// most nine fractional digits, kept in billionths, so `"0.7"` of 300 is 210, and `"0.5"` and
/// `"0.50"` are one share.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Share(u32);

impl Share {
    /// None of an amount.
    pub const NONE: Self = Self(0);

    /// All of an amount.
    pub const ALL: Self = Self(Self::WHOLE);

    /// The most fractional digits a share holds.
    const DIGITS: u32 = 9;

    /// All of an amount in billionths, the parts of a share of nine digits.
    const WHOLE: u32 = 10_u32.pow(Self::DIGITS);

    /// The share of `amount`, rounded down to a whole amount, exactly.
    pub fn of(self, amount: i64) -> i64 {
        let share = i128::from(amount) * i128::from(self.0);
        let share = Rounding::Floor.divide(share, i128::from(Self::WHOLE));
        i64::try_from(share).expect("a share of at most 1 of an amount fits")
    }

    /// The share of `value`, rounded down to the least amount a `Num` holds, exactly.
    pub fn of_num(self, value: Num) -> Num {
        value
            .checked_mul_div_int(self.0.into(), Self::WHOLE.into(), Rounding::Floor)
            .expect("a share of at most 1 of a value fits")
    }

    /// Whether `part` is at most this share of `whole`, decided exactly, as a unit at 35 of 100
    /// life is at most 0.35 of it.
    pub fn covers(self, part: Num, whole: Num) -> bool {
        let part = i128::from(part.to_bits()) * i128::from(Self::WHOLE);
        part <= i128::from(self.0) * i128::from(whole.to_bits())
    }

    /// The share of `parts` billionths; `None` past 1.
    pub const fn billionths(parts: u32) -> Option<Self> {
        if parts > Self::WHOLE {
            return None;
        }
        Some(Self(parts))
    }

    /// Whether it is no share at all.
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// The share `text` writes, `[0|1][.digits]`; `None` past 1 or for anything else.
    fn parse(text: &str) -> Option<Self> {
        let (int, frac) = text.split_once('.').unwrap_or((text, ""));
        let digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
        let places = u32::try_from(frac.len())
            .ok()
            .filter(|&len| len <= Self::DIGITS);
        let places = places.filter(|_| matches!(int, "0" | "1") && digits(frac))?;
        if text.contains('.') && frac.is_empty() {
            return None;
        }
        let scale = 10_u32.pow(Self::DIGITS - places);
        let frac: u32 = if frac.is_empty() {
            0
        } else {
            frac.parse().ok()?
        };
        let parts = u32::from(int == "1") * Self::WHOLE + frac * scale;
        (parts <= Self::WHOLE).then_some(Self(parts))
    }
}

/// The share a decimal text writes, as data writes it in a string: `[0|1][.digits]`, at most nine
/// digits, from 0 to 1.
impl FromStr for Share {
    type Err = ShareError;

    fn from_str(text: &str) -> Result<Self, ShareError> {
        Self::parse(text).ok_or(ShareError)
    }
}

/// As data writes it, what `Deserialize` reads: the integer 0 or 1 for no share or all, else the
/// decimal string of its billionths, of no trailing zero.
impl Serialize for Share {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.0 {
            0 => serializer.serialize_u8(0),
            Self::WHOLE => serializer.serialize_u8(1),
            parts => {
                let digits = format!("{parts:09}");
                serializer.collect_str(&format_args!("0.{}", digits.trim_end_matches('0')))
            }
        }
    }
}

/// The integer 0 or 1, or a decimal string from 0 to 1, as `Share::parse` reads it.
impl<'de> Deserialize<'de> for Share {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(untagged)]
        enum Written {
            Int(u8),
            Text(String),
        }
        let share = match Written::deserialize(deserializer)? {
            Written::Int(int @ (0 | 1)) => Some(Self(u32::from(int) * Self::WHOLE)),
            Written::Int(_) => None,
            Written::Text(text) => Self::parse(&text),
        };
        share.ok_or_else(|| {
            D::Error::custom("a share is 0, 1, or a decimal from 0 to 1 of at most 9 digits")
        })
    }
}

#[cfg(test)]
mod tests;
