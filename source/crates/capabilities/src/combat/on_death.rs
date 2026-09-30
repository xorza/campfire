use bevy_ecs::component::Component;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

/// What becomes of a unit when it dies, as its unit type says.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OnDeath {
    /// It stays, dead, for the mode to respawn, as heroes do.
    Stay,
    /// It despawns, as creeps and towers do.
    Despawn,
}

impl SimComponent for OnDeath {
    const NAME: &'static str = "combat.on_death";
}
