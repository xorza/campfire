//! Deterministic state and systems on `bevy_ecs`, with no genre code.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]
#![allow(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

mod entity_index;
mod id_allocator;
mod position;
mod sim_rng;
mod sim_state;
mod sim_tick;
mod sim_update;
mod stable_id;
mod state_registry;
mod tick_inputs;

pub use entity_index::EntityIndex;
pub use id_allocator::IdAllocator;
pub use position::Position;
pub use sim_rng::SimRng;
pub use sim_state::{SimComponent, SimResource};
pub use sim_tick::SimTick;
pub use sim_update::{SimSet, SimUpdate};
pub use stable_id::StableId;
pub use state_registry::error::SnapshotError;
pub use state_registry::{StateHash, StateRegistry, TypeHash};
pub use tick_inputs::{TickInput, TickInputs};

#[cfg(feature = "bench")]
pub mod bench {
    pub use crate::state_registry::bench::state_hash;
}
