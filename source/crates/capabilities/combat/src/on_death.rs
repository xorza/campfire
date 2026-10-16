use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_declare::PerType;
use campfire_sim::SimComponent;
use campfire_state_types::{DataKind, Replication, SentOnce};
use campfire_units::UnitEntity;
use serde::{Deserialize, Serialize};

/// What becomes of a unit when it dies, as its unit type says: in data, `stay` or `despawn`.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[component(immutable)]
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
    type Host = UnitEntity;

    // Each of its cases is a rule of the engine.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for OnDeath {
    const KIND: DataKind = DataKind::Prediction;
    type Sending = SentOnce;
}

impl PerType for OnDeath {
    const SCRIPTS_ONLY: bool = false;
}
