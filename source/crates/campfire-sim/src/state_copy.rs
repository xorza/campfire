use bevy_ecs::world::{Mut, World};

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
    /// The copy `world` holds, `delta` the last copy into it.
    pub(crate) const fn of(world: World, delta: StateDelta) -> StateCopy {
        StateCopy { world, delta }
    }

    /// Brings the copy up to the state of `world`, the world it copies.
    pub fn follow(&mut self, world: &mut World) {
        let StateCopy { world: copy, delta } = self;
        world.resource_scope(|world, registry: Mut<'_, StateRegistry>| {
            registry.changes(world, delta);
            registry.apply(delta, copy);
        });
    }

    pub const fn world(&self) -> &World {
        &self.world
    }
}
