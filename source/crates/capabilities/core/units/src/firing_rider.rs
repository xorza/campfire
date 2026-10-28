use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use campfire_states::{Replication, SentOnInsert};
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::unit_entity::UnitEntity;

/// Marks a rider that fires from its container, out of the world as every rider is: the scan and
/// the attacks name it, by `Acting`, beside the units in the world, while no search for a target
/// finds it (transports.md, A container may let its riders fire). Interaction sets it as the
/// rider enters and takes it as the rider leaves: interaction's state, kept here, as `Leaving`
/// is, so the attacks name it with no dependency on interaction.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[component(immutable)]
pub struct FiringRider;

impl SimComponent for FiringRider {
    const NAME: &'static str = "units.firing_rider";
    type Host = UnitEntity;

    // A mark, which holds nothing.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for FiringRider {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentOnInsert;
}
