use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::{SimComponent, StableId};
use campfire_states::{Replication, SentPredicted};
use campfire_units::UnitEntity;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

/// The producer and its place a unit was made at, which it holds while it lives
/// (04-capabilities/production.md, A jet holds a place).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MadeAt {
    pub producer: StableId,
    pub place: u8,
}

impl SimComponent for MadeAt {
    const NAME: &'static str = "production.made_at";
    type Host = UnitEntity;

    // Any producer and place: the producer may be gone.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for MadeAt {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}
