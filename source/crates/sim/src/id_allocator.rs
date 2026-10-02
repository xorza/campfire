use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
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

    pub fn allocate(&mut self) -> StableId {
        let id = StableId::new(self.next);
        self.next = self.next.checked_add(1).expect("stable ids exhausted");
        id
    }
}

impl SimResource for IdAllocator {
    const NAME: &'static str = "sim.id_allocator";

    // The restore checks it against the ids in use.
    fn check(&self, _: &World) -> bool {
        true
    }
}
