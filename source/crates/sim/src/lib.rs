//! Deterministic state and systems on `bevy_ecs`, with no genre code.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod entity_index;
mod id_allocator;
mod sim_state;
mod stable_id;
mod state_hasher;

pub use entity_index::EntityIndex;
pub use id_allocator::IdAllocator;
pub use sim_state::{SimComponent, SimResource};
pub use stable_id::StableId;
pub use state_hasher::{StateHash, StateHasher, TypeHash};

#[cfg(feature = "bench")]
pub mod bench {
    pub use crate::state_hasher::bench::state_hash;
}
