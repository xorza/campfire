use bevy_ecs::component::Component;
use campfire_math::Tick;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

/// When a dead unit comes back: at the start of tick `at`, at its spawn point, with full health.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Respawn {
    pub at: Tick,
}

impl SimComponent for Respawn {
    const NAME: &'static str = "combat.respawn";
}
