use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use campfire_states::{Replication, SentOnInsert};
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::unit_entity::UnitEntity;

/// Marks a rider that sees from its container, out of the world as every rider is: vision names
/// it, by `Sighted`, beside the units in the world (transports.md, A garrison's riders see from
/// it). Interaction sets it as the rider enters and takes it as the rider leaves: interaction's
/// state, kept here, as `FiringRider` is, so vision names it with no dependency on interaction.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[component(immutable)]
pub struct SeeingRider;

impl SimComponent for SeeingRider {
    const NAME: &'static str = "units.seeing_rider";
    type Host = UnitEntity;

    // A mark, which holds nothing.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for SeeingRider {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentOnInsert;
}
