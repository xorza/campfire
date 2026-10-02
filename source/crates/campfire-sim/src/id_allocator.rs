use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_common::Tick;
use serde::{Deserialize, Serialize};

use crate::sim_state::SimResource;
use crate::stable_id::StableId;

/// Hands out stable ids in order. An id is never reused, so a despawned entity's id keeps its
/// meaning; the next id is state, since two worlds that differ in it diverge at the next spawn.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdAllocator {
    next: u64,
}

impl IdAllocator {
    /// Whether `id` was handed out, so the allocator will never hand it out again.
    pub(crate) const fn issued(&self, id: StableId) -> bool {
        id.get() < self.next
    }

    pub const fn allocate(&mut self) -> StableId {
        let id = StableId::new(self.next);
        self.next = self.next.checked_add(1).expect("stable ids exhausted");
        id
    }
}

impl SimResource for IdAllocator {
    const NAME: &'static str = "sim.id_allocator";

    // The restore checks it against the ids in use.
    fn check(&self, _: &World) -> bool {
        self.next <= Tick::LIMIT.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_restored_allocator_is_at_most_the_limit_and_allocates_from_it() {
        let world = World::new();
        let mut last = IdAllocator {
            next: Tick::LIMIT.get(),
        };
        assert!(last.check(&world));
        assert_eq!(last.allocate(), StableId::new(Tick::LIMIT.get()));
        assert!(!last.check(&world));
    }
}
