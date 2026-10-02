use std::collections::BTreeMap;
use std::str::FromStr;

use campfire_math::Num;
use campfire_script::rhai::Dynamic;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::values::stat::Stat;

/// A script value in data: a TOML integer, or a decimal string, which is a `Num`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Scalar {
    Int(i64),
    Decimal(#[serde(deserialize_with = "decimal")] Num),
}

impl Scalar {
    pub fn to_num(self) -> Option<Num> {
        match self {
            Scalar::Int(value) => Num::from_int(value),
            Scalar::Decimal(value) => Some(value),
        }
    }

    /// A number as data writes it, read as a `Num`: an integer past what a `Num` holds fails
    /// the read, where the data enters.
    pub(crate) fn num<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Num, D::Error> {
        let scalar = Scalar::deserialize(deserializer)?;
        scalar
            .to_num()
            .ok_or_else(|| D::Error::custom(format!("{scalar:?} is past what a number holds")))
    }

    /// Numbers by stat, each read as `num` reads it.
    pub(crate) fn nums<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<BTreeMap<Stat, Num>, D::Error> {
        let scalars = BTreeMap::<Stat, Scalar>::deserialize(deserializer)?;
        scalars
            .into_iter()
            .map(|(stat, scalar)| {
                let num = scalar.to_num().ok_or_else(|| {
                    D::Error::custom(format!("{stat}: {scalar:?} is past what a number holds"))
                })?;
                Ok((stat, num))
            })
            .collect()
    }

    /// The value as a script sees it: an integer, or a `Num`.
    pub(crate) fn to_dynamic(self) -> Dynamic {
        match self {
            Scalar::Int(value) => Dynamic::from_int(value),
            Scalar::Decimal(value) => Dynamic::from(value),
        }
    }
}

/// A `Num` from a decimal string, such as `"3.5"`: data never holds a float.
fn decimal<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Num, D::Error> {
    let text = String::deserialize(deserializer)?;
    Num::from_str(&text).map_err(Error::custom)
}
