use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::{SimComponent, StableId};
use campfire_states::{Replication, SentOnce};
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::unit_entity::UnitEntity;

/// A spawn's spawner, by stable id: a kept spawn comes back when it dies and dies with its
/// spawner, and a leader's spawn is its leader's (04-capabilities/production.md, A building may
/// keep spawns; 04-capabilities/production.md, A unit may lead its spawns). State every client
/// receives once, as a client picks a spawn as its leader.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[component(immutable)]
#[serde(transparent)]
pub struct SpawnOf {
    spawner: StableId,
}

impl SpawnOf {
    pub const fn new(spawner: StableId) -> Self {
        Self { spawner }
    }

    pub const fn spawner(self) -> StableId {
        self.spawner
    }
}

impl SimComponent for SpawnOf {
    const NAME: &'static str = "units.spawn_of";
    type Host = UnitEntity;

    // Its spawner may be gone, as a spawn outlives it until the next tick's deaths.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for SpawnOf {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentOnce;
}
