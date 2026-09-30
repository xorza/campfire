use serde::Deserialize;

use crate::units::scalar::Scalar;

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
    pub param: String,
}

impl Number {
    /// The param it reads, if it reads one.
    pub fn param(&self) -> Option<&str> {
        match self {
            Number::Value(_) => None,
            Number::Param(reference) => Some(&reference.param),
        }
    }
}
