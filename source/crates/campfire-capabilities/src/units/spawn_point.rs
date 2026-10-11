use crate::state_types::data_kind::DataKind;
use crate::state_types::kinded::Kinded;
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::{Position, SimComponent};
use serde::{Deserialize, Serialize};

/// Where a unit spawned, and where it comes back when it respawns.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

    // Its decode keeps it within the bound.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Kinded for SpawnPoint {
    const KIND: DataKind = DataKind::Prediction;
}
