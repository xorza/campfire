use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use campfire_state_types::{DataKind, Replication, SentPredicted};
use serde::{Deserialize, Serialize};

use crate::units::unit_entity::UnitEntity;

/// Marks a unit that died and stays for the mode to respawn. A dead unit takes no orders, does not
/// move and is no target.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dead;

impl SimComponent for Dead {
    const NAME: &'static str = "units.dead";
    type Host = UnitEntity;

    // A mark, which holds nothing.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for Dead {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}
