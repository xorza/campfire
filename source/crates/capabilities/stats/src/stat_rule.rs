use campfire_math::Num;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

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
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            min: Option<Num>,
            max: Option<Num>,
        }
        let Fields { min, max } = Fields::deserialize(deserializer)?;
        if let (Some(min), Some(max)) = (min, max)
            && min > max
        {
            return Err(D::Error::custom("a stat's min passes its max"));
        }
        Ok(StatRule { min, max })
    }
}
