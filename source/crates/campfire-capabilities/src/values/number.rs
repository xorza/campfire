use campfire_math::Num;
use serde::Deserialize;

use crate::values::declared_name::DeclaredName;
use crate::values::scalar::Scalar;

/// A number field of data: a value, or `{ param = "<name>" }`, which reads the param of that name
/// as `ctx.p` does.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Number {
    Value(Scalar),
    Param(ParamRef),
}

/// `{ param = "<name>" }`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParamRef {
    pub param: DeclaredName,
}

impl Number {
    /// The param it reads, if it reads one.
    pub const fn param(&self) -> Option<&DeclaredName> {
        match self {
            Number::Value(_) => None,
            Number::Param(reference) => Some(&reference.param),
        }
    }
}

impl From<Scalar> for Number {
    fn from(value: Scalar) -> Number {
        Number::Value(value)
    }
}

impl From<i64> for Number {
    fn from(value: i64) -> Number {
        Number::Value(Scalar::Int(value))
    }
}

impl From<Num> for Number {
    fn from(value: Num) -> Number {
        Number::Value(Scalar::Decimal(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_is_its_number() {
        assert_eq!(Number::from(Scalar::Int(3)), Number::Value(Scalar::Int(3)));
        assert_eq!(Number::from(1000), Number::Value(Scalar::Int(1000)));
        assert_eq!(
            Number::from(Num::HALF),
            Number::Value(Scalar::Decimal(Num::HALF))
        );
    }
}
