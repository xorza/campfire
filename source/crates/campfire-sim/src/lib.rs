//! Deterministic state and systems on `bevy_ecs`, with no genre code.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]
#![allow(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

mod capability;
mod command;
mod entity_index;
mod id_allocator;
mod ordered;
mod position;
mod sim_rng;
mod sim_state;
mod sim_tick;
mod sim_update;
mod slot_event;
mod stable_id;
mod state_changes;
mod state_copy;
mod state_registry;
mod tick_inputs;
mod tick_rate;
mod unpredicted;

pub use crate::capability::Capability;
pub use crate::command::Command;
pub use crate::entity_index::EntityIndex;
pub use crate::id_allocator::IdAllocator;
pub use crate::ordered::{Keyed, Ordered};
pub use crate::position::Position;
pub use crate::sim_rng::SimRng;
pub use crate::sim_state::{SimComponent, SimResource};
pub use crate::sim_tick::SimTick;
pub use crate::sim_update::{SimEdge, SimSet, SimUpdate};
pub use crate::slot_event::{SlotEvent, SlotEventKind};
pub use crate::stable_id::StableId;
pub use crate::state_copy::StateCopy;
pub use crate::state_registry::error::SnapshotError;
pub use crate::state_registry::state_delta::StateDelta;
pub use crate::state_registry::{StateRegistry, TypeHash};
pub use crate::tick_inputs::{PlayerCommand, TickInput, TickInputs};
pub use crate::tick_rate::TickRate;
pub use crate::unpredicted::Unpredicted;

#[cfg(any(test, feature = "internals"))]
pub mod internals {
    pub use crate::sim_update::stage_clock::StageClock;
    pub use crate::state_registry::internals::Draws;
}

#[cfg(feature = "bench")]
pub mod bench {
    use criterion::Criterion;

    use crate::state_registry;

    /// Runs each bench of the crate whose id criterion's filter takes.
    pub fn run(c: &mut Criterion) {
        state_registry::bench::state_hash(c);
        state_registry::bench::snapshot(c);
    }
}
