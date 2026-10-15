use std::any::TypeId;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::{HostRule, SimComponent, StableId};
use campfire_state_types::{DataKind, Replication, SentOnce};
use serde::{Deserialize, Serialize};

use crate::units::unit_entity::body_unit::BodyUnit;
use crate::units::unit_entity::delivery_entity::DeliveryEntity;

pub(crate) mod body_unit;
pub(crate) mod delivery_entity;

/// Every unit, a delivery among them: the group of the two kinds of unit, `BodyUnit` and
/// `DeliveryEntity`, which each requires. It is state, as the restore checks of what every unit
/// holds name it.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[component(immutable)]
pub struct UnitEntity;

/// A unit of either kind hosts the state of every unit.
impl HostRule for UnitEntity {
    fn holds(world: &World, entity: Entity) -> bool {
        world.get::<UnitEntity>(entity).is_some()
    }

    fn kinds(kinds: &mut Vec<TypeId>) -> bool {
        kinds.push(TypeId::of::<BodyUnit>());
        kinds.push(TypeId::of::<DeliveryEntity>());
        true
    }
}

impl SimComponent for UnitEntity {
    const NAME: &'static str = "units.unit";
    type Host = UnitEntity;

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
