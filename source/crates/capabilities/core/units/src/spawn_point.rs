use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::{Position, SimComponent};
use campfire_state_types::{DataKind, Replication, SentOnce};
use serde::{Deserialize, Serialize};

use crate::map_book::MapBook;
use crate::unit_entity::UnitEntity;

/// Where a unit spawned, and where it comes back when it respawns.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[component(immutable)]
#[serde(transparent)]
pub struct SpawnPoint(Position);

impl SpawnPoint {
    pub const fn new(pos: Position) -> SpawnPoint {
        SpawnPoint(pos)
    }

    pub const fn get(self) -> Position {
        self.0
    }
}

impl SimComponent for SpawnPoint {
    const NAME: &'static str = "units.spawn_point";
    type Host = UnitEntity;

    // A unit respawns where it spawned, so the point lies within the map's bounds, as its
    // place does.
    fn check(&self, world: &World, _: Entity) -> bool {
        world.resource::<MapBook>().bounds.contains(self.0)
    }
}

impl Replication for SpawnPoint {
    const KIND: DataKind = DataKind::Prediction;
    type Sending = SentOnce;
}
