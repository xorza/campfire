use std::collections::BTreeMap;

use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;

use crate::stable_id::StableId;

/// Live entities by stable id, in id order: the order every hash and snapshot walks, without
/// sorting each tick. The `StableId` hooks keep it current.
#[derive(Resource, Debug, Default)]
pub struct EntityIndex {
    entities: BTreeMap<StableId, Entity>,
}

impl EntityIndex {
    pub fn get(&self, id: StableId) -> Option<Entity> {
        self.entities.get(&id).copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = (StableId, Entity)> + '_ {
        self.entities.iter().map(|(&id, &entity)| (id, entity))
    }

    pub(crate) fn insert(&mut self, id: StableId, entity: Entity) -> Option<Entity> {
        self.entities.insert(id, entity)
    }

    pub(crate) fn remove(&mut self, id: StableId) -> Option<Entity> {
        self.entities.remove(&id)
    }
}
