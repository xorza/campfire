use campfire_common::PlayerSlot;
use campfire_math::Num;
use campfire_sim::{Position, StableId};

use crate::units::owner::Owner;
use crate::units::team::Team;
use crate::units::unit_core::UnitCore;
use crate::units::unit_type::UnitType;

/// A unit to spawn: the id it takes, its unit type, its team, the player that owns it, if one
/// does, where, and the angle in degrees it faces and a box body turns by, which a circle ignores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SpawnAt {
    pub(crate) id: StableId,
    pub(crate) unit_type: UnitType,
    pub(crate) team: Team,
    pub(crate) owner: Option<PlayerSlot>,
    pub(crate) pos: Position,
    pub(crate) angle: Num,
}

impl SpawnAt {
    /// The core of the unit.
    pub(crate) fn core(self) -> UnitCore {
        UnitCore {
            id: self.id,
            pos: self.pos,
            unit_type: self.unit_type,
            team: self.team,
            owner: self.owner.map(Owner::new),
        }
    }
}
