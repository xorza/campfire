use bevy_ecs::component::Component;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

/// What becomes of a unit when it dies, as its unit type says: in data, `stay` or `despawn`.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnDeath {
    /// It stays, dead, for the mode to respawn, as avatars do.
    Stay,
    /// It despawns, as creeps and towers do.
    #[default]
    Despawn,
}

impl SimComponent for OnDeath {
    const NAME: &'static str = "combat.on_death";
}
