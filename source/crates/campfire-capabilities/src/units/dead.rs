use crate::state_types::data_kind::DataKind;
use crate::state_types::replication::Replication;
use crate::state_types::sending::SentPredicted;
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

/// Marks a unit that died and stays for the mode to respawn. A dead unit takes no orders, does not
/// move and is no target.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dead;

impl SimComponent for Dead {
    const NAME: &'static str = "units.dead";

    // A mark, which holds nothing.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for Dead {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}
