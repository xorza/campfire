use std::fmt;
use std::str::FromStr;

use campfire_math::{Num, ParseNumError};
use serde::{Deserialize, Serialize};

/// A script value in data: a TOML integer, or a decimal string, which is a `Num`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Scalar {
    Int(i64),
    Decimal(Num),
}

impl Scalar {
    pub const fn to_num(self) -> Option<Num> {
        match self {
            Self::Int(value) => Num::from_int(value),
            Self::Decimal(value) => Some(value),
        }
    }
}

/// As data writes it: an integer, or a decimal's exact value.
impl fmt::Display for Scalar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int(value) => write!(f, "{value}"),
            Self::Decimal(value) => write!(f, "{value}"),
        }
    }
}

/// An integer as one, and any other decimal as a `Num`, as data reads `5` and `"0.5"`.
impl FromStr for Scalar {
    type Err = ParseNumError;

    fn from_str(text: &str) -> Result<Self, ParseNumError> {
        match text.parse() {
            Ok(value) => Ok(Self::Int(value)),
            Err(_) => text.parse().map(Self::Decimal),
        }
    }
}

impl From<i64> for Scalar {
    fn from(value: i64) -> Self {
        Self::Int(value)
    }
}

impl From<Num> for Scalar {
    fn from(value: Num) -> Self {
        Self::Decimal(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_integer_or_a_number_is_its_scalar() {
        assert_eq!(Scalar::from(-5), Scalar::Int(-5));
        assert_eq!(Scalar::from(Num::HALF), Scalar::Decimal(Num::HALF));
        // A text reads as an integer, else a decimal, and writes back as data writes it.
        let read = |text: &str| text.parse::<Scalar>();
        assert_eq!(read("50000"), Ok(Scalar::Int(50000)));
        assert_eq!(read("-0.5"), Ok(Scalar::Decimal(-Num::HALF)));
        assert_eq!(read("five"), Err(ParseNumError::Malformed));
        assert_eq!(Scalar::Int(50000).to_string(), "50000");
        assert_eq!(Scalar::Decimal(Num::HALF).to_string(), "0.5");
    }
}
