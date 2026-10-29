use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use campfire_states::{Replication, SentOnce};
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::unit_entity::UnitEntity;

/// Marks a unit that leads its spawns: of no life, body or walk of its own, it stands at the
/// centre of its living spawns, an order to it is theirs, and it dies with the last
/// (04-capabilities/production.md, A unit may lead its spawns). State every client receives once.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[component(immutable)]
pub struct Leader;

impl SimComponent for Leader {
    const NAME: &'static str = "units.leader";
    type Host = UnitEntity;

    // Its type leads its spawns, which the check of its kept spawns, production's, holds.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for Leader {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentOnce;
}
