use campfire_common::PlayerSlot;
use campfire_math::{Num, Vec3};
use campfire_sim::StableId;
use campfire_values::{Team, UnitType};

use crate::owner::Owner;
use crate::unit_core::UnitCore;
use crate::variation::Variation;

/// A unit to spawn: the id it takes, its unit type, its team, the player that owns it, if one
/// does, where, the angle in degrees it faces and a box body turns by, which a circle ignores,
/// and its variation, when its spawn takes a given one in place of a draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpawnAt {
    pub id: StableId,
    pub unit_type: UnitType,
    pub team: Team,
    pub owner: Option<PlayerSlot>,
    pub position: Vec3,
    pub angle: Num,
    pub variation: Option<Variation>,
}

impl SpawnAt {
    /// The core of the unit.
    pub const fn core(self) -> UnitCore {
        let owner = match self.owner {
            Some(slot) => Some(Owner::new(slot)),
            None => None,
        };
        UnitCore {
            id: self.id,
            position: self.position,
            unit_type: self.unit_type,
            team: self.team,
            owner,
            angle: self.angle,
            variation: self.variation,
        }
    }
}
