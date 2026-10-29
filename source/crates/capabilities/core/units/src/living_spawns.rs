use std::slice;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::{EntityIndex, SimComponent, StableId};
use campfire_states::{Replication, SentOnChange};
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::spawn_of::SpawnOf;
use crate::unit_entity::UnitEntity;

/// The spawns of a spawner that live, by stable id, in the order they spawned, so in increasing
/// stable id: those a kept spawner's death takes, and a leader's order passes to and its place
/// is the centre of (04-capabilities/production.md, A unit may lead its spawns). State its owner's
/// prediction reads, as it passes its orders.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LivingSpawns {
    spawns: Vec<StableId>,
}

impl LivingSpawns {
    /// An empty list, of room for `count` spawns.
    pub fn with_capacity(count: usize) -> Self {
        Self {
            spawns: Vec::with_capacity(count),
        }
    }

    /// Adds `spawn`, which spawned after each it holds.
    pub fn push(&mut self, spawn: StableId) {
        debug_assert!(
            self.spawns.last().is_none_or(|&last| last < spawn),
            "a spawn spawns after each that lives"
        );
        self.spawns.push(spawn);
    }

    /// Removes `spawn`; whether it held it.
    pub fn remove(&mut self, spawn: StableId) -> bool {
        let Ok(at) = self.spawns.binary_search(&spawn) else {
            return false;
        };
        self.spawns.remove(at);
        true
    }

    /// Empties it.
    pub fn clear(&mut self) {
        self.spawns.clear();
    }

    pub fn contains(&self, spawn: StableId) -> bool {
        self.spawns.binary_search(&spawn).is_ok()
    }

    pub const fn len(&self) -> usize {
        self.spawns.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.spawns.is_empty()
    }

    pub fn iter(&self) -> slice::Iter<'_, StableId> {
        self.spawns.iter()
    }
}

impl<'a> IntoIterator for &'a LivingSpawns {
    type Item = &'a StableId;
    type IntoIter = slice::Iter<'a, StableId>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl SimComponent for LivingSpawns {
    const NAME: &'static str = "units.living_spawns";
    type Host = UnitEntity;

    // A restore's untrusted list holds each spawn once, in increasing stable id, and only units
    // that are its unit's spawns.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let Some(&spawner) = world.get::<StableId>(entity) else {
            return false;
        };
        let index = world.resource::<EntityIndex>();
        let of_it = |&spawn: &StableId| {
            let spawn_of = index
                .get(spawn)
                .and_then(|spawn| world.get::<SpawnOf>(spawn));
            spawn_of.is_some_and(|spawn_of| spawn_of.spawner() == spawner)
        };
        self.spawns.is_sorted_by(|a, b| a < b) && self.spawns.iter().all(of_it)
    }
}

impl Replication for LivingSpawns {
    const KIND: DataKind = DataKind::Prediction;
    type Sending = SentOnChange;
}
