use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::values::filter_data::FilterData;

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
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Targeting, D::Error> {
        let text = String::deserialize(deserializer)?;
        match text.as_str() {
            "none" => Ok(Targeting::None),
            "point" => Ok(Targeting::Point),
            "direction" => Ok(Targeting::Direction),
            filter => FilterData::parse(filter)
                .map(Targeting::Unit)
                .ok_or_else(|| Error::custom(format!("unknown targeting {filter:?}"))),
        }
    }
}
