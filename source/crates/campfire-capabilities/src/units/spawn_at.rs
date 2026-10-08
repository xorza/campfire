use campfire_math::Num;
use campfire_sim::{Position, StableId};

use crate::units::team::Team;
use crate::units::unit_type::UnitType;

/// A unit to spawn: the id it takes, its unit type, its team, where, and the angle in degrees a
/// box body turns by, which a circle ignores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SpawnAt {
    pub(crate) id: StableId,
    pub(crate) unit_type: UnitType,
    pub(crate) team: Team,
    pub(crate) pos: Position,
    pub(crate) angle: Num,
}
