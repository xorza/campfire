use campfire_math::Num;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::values::filter_data::FilterData;
use crate::values::scalar::Scalar;

/// A build's `placement`: for each entry of `near`, its box must come within `distance` of a
/// living unit the filter selects for the builder; for each of `away`, of none.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacementData {
    #[serde(default)]
    pub near: Vec<PlacementRule>,
    #[serde(default)]
    pub away: Vec<PlacementRule>,
}

/// An entry of a placement: a filter, and a distance in meters, 0 or more.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacementRule {
    pub filter: FilterData,
    pub distance: Num,
}

/// `{ filter, distance }`, the distance a number from 0.
impl<'de> Deserialize<'de> for PlacementRule {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<PlacementRule, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            filter: FilterData,
            distance: Scalar,
        }
        let Fields { filter, distance } = Fields::deserialize(deserializer)?;
        let distance = distance
            .to_num()
            .filter(|&distance| distance >= Num::ZERO)
            .ok_or_else(|| D::Error::custom("a placement's distance is a number from 0"))?;
        Ok(PlacementRule { filter, distance })
    }
}
