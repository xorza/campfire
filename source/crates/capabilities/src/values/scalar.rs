use std::collections::BTreeMap;
use std::str::FromStr;

use campfire_math::Num;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::values::ranked::Ranked;
use crate::values::stat::Stat;

/// A script value in data: a TOML integer, or a decimal string, which is a `Num`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Scalar {
    Int(i64),
    Decimal(#[serde(deserialize_with = "decimal")] Num),
}

impl Scalar {
    pub const fn to_num(self) -> Option<Num> {
        match self {
            Scalar::Int(value) => Num::from_int(value),
            Scalar::Decimal(value) => Some(value),
        }
    }

    /// A number as data writes it, read as a `Num`: an integer past what a `Num` holds fails
    /// the read, where the data enters.
    pub(crate) fn num<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Num, D::Error> {
        Scalar::deserialize(deserializer)?.checked("")
    }

    /// Numbers by stat, each read as `num` reads it.
    pub(crate) fn nums<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<BTreeMap<Stat, Num>, D::Error> {
        let scalars = BTreeMap::<Stat, Scalar>::deserialize(deserializer)?;
        scalars
            .into_iter()
            .map(|(stat, scalar)| {
                let num = scalar.checked(&format!("{stat}: "))?;
                Ok((stat, num))
            })
            .collect()
    }

    /// One number or one per rank, each read as `num` reads it.
    pub(crate) fn ranked_num<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Ranked<Num>, D::Error> {
        Ok(match Ranked::<Scalar>::deserialize(deserializer)? {
            Ranked::One(scalar) => Ranked::One(scalar.checked("")?),
            Ranked::PerRank(scalars) => Ranked::PerRank(
                scalars
                    .into_iter()
                    .map(|scalar| scalar.checked(""))
                    .collect::<Result<_, _>>()?,
            ),
        })
    }

    /// The value as a `Num`; past what one holds, an error of the read, after `at`.
    pub(crate) fn checked<E: Error>(self, at: &str) -> Result<Num, E> {
        self.to_num()
            .ok_or_else(|| E::custom(format!("{at}{self:?} is past what a number holds")))
    }
}

/// A `Num` from a decimal string, such as `"3.5"`: data never holds a float.
fn decimal<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Num, D::Error> {
    let text = String::deserialize(deserializer)?;
    Num::from_str(&text).map_err(Error::custom)
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
