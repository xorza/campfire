use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

/// A unit its AI reset: it walks to its spawn place and takes no order until it arrives, when its
/// pools are full again.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Resetting;

impl SimComponent for Resetting {
    const NAME: &'static str = "orders.resetting";

    // A mark, which holds nothing.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}
