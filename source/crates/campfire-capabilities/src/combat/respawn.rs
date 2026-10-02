use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

/// When a dead unit comes back: at the start of tick `at`, at its spawn point, with full health.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Respawn {
    pub at: Tick,
}

impl SimComponent for Respawn {
    const NAME: &'static str = "combat.respawn";

    fn check(&self, _: &World, _: Entity) -> bool {
        self.at <= Tick::LIMIT
    }
}
