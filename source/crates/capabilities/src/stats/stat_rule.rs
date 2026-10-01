use campfire_math::Num;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::values::scalar::Scalar;

/// The limits a stat's value keeps within, as the mode declares them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StatRule {
    pub min: Option<Num>,
    pub max: Option<Num>,
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
            min: Option<Scalar>,
            max: Option<Scalar>,
        }
        let Fields { min, max } = Fields::deserialize(deserializer)?;
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
        Ok(StatRule { min, max })
    }
}
