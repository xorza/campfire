use campfire_math::Num;
use campfire_sim::Position;

use crate::navigation::path_walker::PathEnd;
use crate::units::path_id::PathId;
use crate::units::team::Team;
use crate::units::unit_type::UnitType;

/// A unit of the map, names resolved: on its path, if it names one, and turned by its angle in
/// degrees if it has a box body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlacedUnit {
    pub(crate) unit_type: UnitType,
    pub(crate) team: Team,
    pub(crate) path: Option<PlacedPath>,
    pub(crate) pos: Position,
    pub(crate) angle: Num,
}

/// The path a placed unit is on, and the end it walks it from, if it walks it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlacedPath {
    pub(crate) path: PathId,
    pub(crate) from: Option<PathEnd>,
}
