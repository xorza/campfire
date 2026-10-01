use campfire_math::Num;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::values::scalar::Scalar;

/// How a stat combines its unit type's value with its modifiers' values, and the limits the
/// result keeps within, as the mode declares it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatRule {
    pub combine: Combine,
    pub min: Option<Num>,
    pub max: Option<Num>,
}

/// How a stat's values combine.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Combine {
    /// The type's value plus each modifier's times its stacks.
    #[default]
    Sum,
    /// The greatest of the type's value and each modifier's times its stacks.
    Highest,
}

impl StatRule {
    /// `value` within the limits.
    pub(crate) fn clamp(self, value: Num) -> Num {
        let value = self.min.map_or(value, |min| value.max(min));
        self.max.map_or(value, |max| value.min(max))
    }
}

/// A rule whose limits are not numbers, or whose minimum passes its maximum, fails to read.
impl<'de> Deserialize<'de> for StatRule {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<StatRule, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(default)]
            combine: Combine,
            min: Option<Scalar>,
            max: Option<Scalar>,
        }
        let Fields { combine, min, max } = Fields::deserialize(deserializer)?;
        let number = |limit: Option<Scalar>| match limit {
            None => Ok(None),
            Some(limit) => limit
                .to_num()
                .map(Some)
                .ok_or_else(|| D::Error::custom("a stat's limit is a number")),
        };
        let (min, max) = (number(min)?, number(max)?);
        if let (Some(min), Some(max)) = (min, max)
            && min > max
        {
            return Err(D::Error::custom("a stat's min passes its max"));
        }
        Ok(StatRule { combine, min, max })
    }
}
