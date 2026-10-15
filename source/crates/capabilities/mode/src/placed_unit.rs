use campfire_math::{Num, Vec3};
use campfire_navigation::PathEnd;
use campfire_values::{PathId, Team, UnitType};

/// A unit of the map, names resolved: on its path, if it names one, and turned by its angle in
/// degrees if it has a box body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlacedUnit {
    pub(crate) unit_type: UnitType,
    pub team: Team,
    pub(crate) path: Option<PlacedPath>,
    pub pos: Vec3,
    pub(crate) angle: Num,
}

/// The path a placed unit is on, and the end it walks it from, if it walks it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlacedPath {
    pub(crate) path: PathId,
    pub(crate) from: Option<PathEnd>,
}
