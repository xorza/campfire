use campfire_math::Num;
use serde::Deserialize;

/// A script value in data: a TOML integer, or a decimal string, which is a `Num`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Scalar {
    Int(i64),
    Decimal(Num),
}

impl Scalar {
    pub const fn to_num(self) -> Option<Num> {
        match self {
            Scalar::Int(value) => Num::from_int(value),
            Scalar::Decimal(value) => Some(value),
        }
    }
}

impl From<i64> for Scalar {
    fn from(value: i64) -> Scalar {
        Scalar::Int(value)
    }
}

impl From<Num> for Scalar {
    fn from(value: Num) -> Scalar {
        Scalar::Decimal(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_integer_or_a_number_is_its_scalar() {
        assert_eq!(Scalar::from(-5), Scalar::Int(-5));
        assert_eq!(Scalar::from(Num::HALF), Scalar::Decimal(Num::HALF));
    }
}
