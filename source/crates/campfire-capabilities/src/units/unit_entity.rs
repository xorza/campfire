use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::{HostRule, OnAny, SimComponent, StableId};
use serde::{Deserialize, Serialize};

use crate::state_types::data_kind::DataKind;
use crate::state_types::replication::Replication;
use crate::state_types::sending::SentOnce;

/// The kind of every unit, a delivery among them. A unit type requires it, so every spawn of a
/// typed unit gives it; it is state too, so a unit of no type, as a test spawns, stays a unit
/// through a restore.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[component(immutable)]
pub struct UnitEntity;

impl HostRule for UnitEntity {
    fn holds(world: &World, entity: Entity) -> bool {
        world.get::<UnitEntity>(entity).is_some()
    }
}

impl SimComponent for UnitEntity {
    const NAME: &'static str = "units.unit";
    type Host = OnAny;

    // A unit of a named id would stand where a player's or the match's entity is looked for, and
    // be of two kinds.
    fn check(&self, world: &World, entity: Entity) -> bool {
        world
            .get::<StableId>(entity)
            .is_some_and(|id| id.is_allocated())
    }
}

impl Replication for UnitEntity {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentOnce;
}
