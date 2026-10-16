use campfire_values::FilterData;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

/// What an action targets. In data: `none`, `point`, `direction`, or a filter of the units it
/// may target, such as `enemies` or `enemies:avatar`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Targeting {
    None,
    Point,
    Direction,
    Unit(FilterData),
}

impl<'de> Deserialize<'de> for Targeting {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        match text.as_str() {
            "none" => Ok(Self::None),
            "point" => Ok(Self::Point),
            "direction" => Ok(Self::Direction),
            filter => FilterData::parse(filter)
                .map(Targeting::Unit)
                .ok_or_else(|| Error::custom(format!("unknown targeting {filter:?}"))),
        }
    }
}
