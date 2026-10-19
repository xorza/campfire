use bevy_ecs::component::Component;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::world::DeferredWorld;
use campfire_common::PlayerSlot;
use serde::{Deserialize, Serialize};

use crate::stable_id::entity_index::EntityIndex;
use crate::stable_id::state_changes::StateChanges;

pub(crate) mod entity_index;
pub(crate) mod state_changes;

/// An entity's identity in the sim, the protocol and replays. Unlike Bevy's `Entity`, it is
/// allocated in order and never reused, or, for an entity a match holds once, named: the match's
/// own and each player's take fixed ids past every id the allocator hands out, so their spawn
/// moves no unit's id, nor a draw or a think keyed by it. It is immutable, so it changes only by
/// an insert, which the hooks keep `EntityIndex` in step with.
#[derive(
    Component, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[component(immutable, on_insert = index_on_insert, on_discard = unindex_on_discard)]
#[serde(transparent)]
pub struct StableId(u64);

/// Its number, by which a geometry index orders it.
impl From<StableId> for u64 {
    fn from(id: StableId) -> Self {
        id.0
    }
}

impl StableId {
    /// The first named id, past `Tick::LIMIT`, which bounds the allocator: the match's entity's.
    pub const MATCH: Self = Self(1 << 63);

    pub(crate) const fn new(value: u64) -> Self {
        Self(value)
    }

    /// The named id of player `slot`'s entity, after the match's.
    pub const fn of_player(slot: PlayerSlot) -> Self {
        Self(Self::MATCH.0 + 1 + slot.get() as u64)
    }

    /// Whether the allocator handed the id out, so it is not named.
    pub const fn is_allocated(self) -> bool {
        self.0 < Self::MATCH.0
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
    if let Some(mut changes) = world.get_resource_mut::<StateChanges>() {
        changes.gain(id);
    }
}

fn unindex_on_discard(mut world: DeferredWorld<'_>, context: HookContext) {
    let id = *world
        .get::<StableId>(context.entity)
        .expect("on_discard runs on an entity with a StableId");
    world.resource_mut::<EntityIndex>().remove(id);
    if let Some(mut changes) = world.get_resource_mut::<StateChanges>() {
        changes.lose(id);
    }
}

#[cfg(test)]
mod tests {
    use bevy_ecs::world::World;

    use super::*;
    use crate::id_allocator::IdAllocator;
    use crate::sim_world::SimWorld;

    #[test]
    fn index_follows_spawn_despawn_and_replace() {
        let mut world = World::new();
        world.init_resource::<EntityIndex>();
        world.init_resource::<IdAllocator>();
        let first = world.resource_mut::<IdAllocator>().allocate();
        let second = world.resource_mut::<IdAllocator>().allocate();
        let entity = world.spawn(first).id();
        assert_eq!(world.entity_of(first), Some(entity));

        world.entity_mut(entity).insert(second);
        assert_eq!(world.entity_of(first), None);
        assert_eq!(world.entity_of(second), Some(entity));

        world.despawn(entity);
        assert_eq!(world.resource::<EntityIndex>().iter().count(), 0);
        // Ids are never reused: the next one follows the last allocated.
        let third = world.resource_mut::<IdAllocator>().allocate();
        assert_eq!(third.get(), 2);
        // The named ids: the match's at 2⁶³, then each player's by slot, none of them allocated;
        // they sort after every allocated id.
        let player = StableId::of_player(PlayerSlot::new(5));
        assert_eq!(
            [StableId::MATCH.get(), player.get()],
            [1 << 63, (1 << 63) + 6]
        );
        assert!(third.is_allocated() && !StableId::MATCH.is_allocated() && !player.is_allocated());
        let named = world.spawn(player).id();
        world.spawn(third);
        assert_eq!(world.entity_of(player), Some(named));
        assert_eq!(
            world.resource::<EntityIndex>().last_allocated(),
            Some(third)
        );
    }
}
