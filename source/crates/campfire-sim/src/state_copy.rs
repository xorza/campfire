use bevy_ecs::world::World;

use crate::entity_index::EntityIndex;
use crate::state_registry::StateRegistry;
use crate::state_registry::state_delta::StateDelta;

/// A world that follows a match's state by copies of what changed: it starts as a full copy,
/// and each `follow` applies the values changed since the last, as a checkpoint's own copy of
/// the state does. It holds the state only: no books, no systems.
#[derive(Debug)]
pub struct StateCopy {
    world: World,
    /// The last copy, kept between copies.
    delta: StateDelta,
}

impl StateCopy {
    /// A copy of the state of `state`, which from then on records its changes for the copy.
    pub fn new(registry: &StateRegistry, state: &mut World) -> StateCopy {
        let mut delta = StateDelta::default();
        registry.track(state, &mut delta);
        let mut world = World::new();
        world.init_resource::<EntityIndex>();
        registry.apply(&delta, &mut world);
        StateCopy { world, delta }
    }

    /// Brings the copy up to the state of `state`, the world it copies.
    pub fn follow(&mut self, registry: &StateRegistry, state: &mut World) {
        registry.changes(state, &mut self.delta);
        registry.apply(&self.delta, &mut self.world);
    }

    pub const fn world(&self) -> &World {
        &self.world
    }
}
