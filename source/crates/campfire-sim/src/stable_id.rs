use bevy_ecs::component::Component;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::world::DeferredWorld;
use serde::{Deserialize, Serialize};

use crate::entity_index::EntityIndex;

/// An entity's identity in the sim, the protocol and replays. Unlike Bevy's `Entity`, it is
/// allocated in order and never reused. It is immutable, so it changes only by an insert, which
/// the hooks keep `EntityIndex` in step with.
#[derive(
    Component, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[component(immutable, on_insert = index_on_insert, on_discard = unindex_on_discard)]
#[serde(transparent)]
pub struct StableId(u64);

impl StableId {
    pub(crate) const fn new(value: u64) -> StableId {
        StableId(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

fn index_on_insert(mut world: DeferredWorld<'_>, context: HookContext) {
    let id = *world
        .get::<StableId>(context.entity)
        .expect("on_insert runs on an entity with a StableId");
    let previous = world
        .resource_mut::<EntityIndex>()
        .insert(id, context.entity);
    debug_assert!(previous.is_none(), "{id:?} is already in use");
}

fn unindex_on_discard(mut world: DeferredWorld<'_>, context: HookContext) {
    let id = *world
        .get::<StableId>(context.entity)
        .expect("on_discard runs on an entity with a StableId");
    world.resource_mut::<EntityIndex>().remove(id);
}

#[cfg(test)]
mod tests {
    use bevy_ecs::world::World;

    use super::*;
    use crate::id_allocator::IdAllocator;

    #[test]
    fn index_follows_spawn_despawn_and_replace() {
        let mut world = World::new();
        world.init_resource::<EntityIndex>();
        world.init_resource::<IdAllocator>();
        let first = world.resource_mut::<IdAllocator>().allocate();
        let second = world.resource_mut::<IdAllocator>().allocate();
        let entity = world.spawn(first).id();
        assert_eq!(world.resource::<EntityIndex>().get(first), Some(entity));

        world.entity_mut(entity).insert(second);
        assert_eq!(world.resource::<EntityIndex>().get(first), None);
        assert_eq!(world.resource::<EntityIndex>().get(second), Some(entity));

        world.despawn(entity);
        assert_eq!(world.resource::<EntityIndex>().iter().count(), 0);
        // Ids are never reused: the next one follows the last allocated.
        assert_eq!(world.resource_mut::<IdAllocator>().allocate().get(), 2);
    }
}
