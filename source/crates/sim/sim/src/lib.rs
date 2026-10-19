//! Deterministic state and systems on `bevy_ecs`, with no genre code.

// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (07-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]
#![allow(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

mod capability;
mod command;
mod due_index;
mod host_rule;
mod id_allocator;
mod match_rate;
mod ordered;
mod own_resources;
mod position;
mod prediction_started;
mod removals;
mod restoring;
mod seat_event;
mod sim_rng;
mod sim_state;
mod sim_tick;
mod sim_update;
mod sim_world;
mod sim_world_builder;
mod stable_id;
mod state_copy;
mod state_registry;
mod state_rewritten;
mod tick_clock;
mod tick_inputs;
mod timed_process;
mod unpredicted;

pub use crate::capability::Capability;
pub use crate::command::Command;
pub use crate::due_index::{DueIndex, DueUnit};
pub use crate::host_rule::{Either, HostRule, KindMarker, OnAny};
pub use crate::id_allocator::IdAllocator;
pub use crate::match_rate::MatchRate;
pub use crate::ordered::{Keyed, Ordered};
pub use crate::own_resources::OwnResources;
pub use crate::position::Position;
pub use crate::prediction_started::PredictionStarted;
pub use crate::removals::{Removals, Removed};
pub use crate::restoring::Restoring;
pub use crate::seat_event::{SeatEvent, SeatEventKind};
pub use crate::sim_rng::{RngOpener, SimRng};
pub use crate::sim_state::{SimComponent, SimResource};
pub use crate::sim_tick::SimTick;
pub use crate::sim_update::{SimEdge, SimSet, SimUpdate};
pub use crate::sim_world::SimWorld;
pub use crate::sim_world_builder::{SimFields, SimWorldBuilder};
pub use crate::stable_id::StableId;
pub use crate::stable_id::entity_index::EntityIndex;
pub use crate::state_copy::StateCopy;
pub use crate::state_registry::breach::Breach;
pub use crate::state_registry::error::SnapshotError;
pub use crate::state_registry::state_delta::StateDelta;
pub use crate::state_registry::state_registry_builder::StateRegistryBuilder;
pub use crate::state_registry::{StateRegistry, StateTypeHash, StateTypeHashMeaning, TypeHash};
pub use crate::state_rewritten::StateRewritten;
pub use crate::tick_clock::TickClock;
pub use crate::tick_inputs::{PlayerCommand, TickInput, TickInputs};
pub use crate::timed_process::{TimedKept, TimedProcess, TimedRuns};
pub use crate::unpredicted::Unpredicted;

#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::sim_update::stage_clock::StageClock;
    #[expect(
        clippy::module_name_repetitions,
        reason = "a module's world trait for tests and benches is `<Module>WorldInternals` (AGENTS.md)"
    )]
    pub use crate::sim_world::internals::SimWorldInternals;
    pub use crate::state_registry::internals::{DrawError, Draws, Mutation};
}

#[cfg(feature = "internals")]
pub mod bench {
    use criterion::Criterion;

    use crate::state_registry;

    /// Runs each bench of the crate whose id criterion's filter takes.
    pub fn run(c: &mut Criterion) {
        state_registry::bench::state_hash(c);
        state_registry::bench::snapshot(c);
        state_registry::bench::state_delta(c);
    }
}
