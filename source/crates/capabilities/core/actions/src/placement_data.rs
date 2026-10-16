use campfire_values::{Distance, FilterData};
use serde::Deserialize;

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
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacementRule {
    pub filter: FilterData,
    pub distance: Distance,
}
