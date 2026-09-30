use bevy_ecs::world::World;
use campfire_sim::{EntityIndex, Position, StableId};

use crate::combat::dead::Dead;
use crate::combat::health::Health;
use crate::combat::team::Team;

/// A unit that may be targeted: alive, with health, a position and a team. Its values are those
/// of the moment it was looked up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LivingUnit {
    pub(crate) id: StableId,
    pub(crate) pos: Position,
    pub(crate) team: Team,
}

impl LivingUnit {
    /// Fills `units` with the living units of `world`, by stable id: the units `Targets` finds.
    pub(crate) fn collect(world: &World, units: &mut Vec<LivingUnit>) {
        units.clear();
        units.extend(
            world
                .resource::<EntityIndex>()
                .iter()
                .filter_map(|(id, entity)| {
                    let unit = world.entity(entity);
                    if unit.contains::<Dead>() || !unit.contains::<Health>() {
                        return None;
                    }
                    Some(LivingUnit {
                        id,
                        pos: *unit.get::<Position>()?,
                        team: *unit.get::<Team>()?,
                    })
                }),
        );
    }
}
