use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_sim::{DueIndex, SimComponent, TimedKept, TimedProcess, TimedRuns};
use campfire_states::{Replication, SentPredicted};
use campfire_units::UnitEntity;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

/// When a dead unit comes back: at the start of tick `at`, at its spawn point, with full health.
/// It is inserted, never changed in place, and its hook pushes its unit into the index of the
/// respawns due at each insert, a mode's, a restore's and a client's rollback's alike.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[component(on_insert = DueIndex::<Self>::pushed)]
pub struct Respawn {
    pub at: Tick,
}

impl Respawn {}

/// A respawn comes due in its tick, on a predicting client too, which brings its own units back.
impl TimedProcess for Respawn {
    const RUNS: TimedRuns = TimedRuns::EveryRole;
    const KEPT: TimedKept = TimedKept::BySetters;
    type Parts = &'static Self;
    type Books = ();

    fn due((): &(), respawn: &Self) -> Option<Tick> {
        Some(respawn.at)
    }
}

impl SimComponent for Respawn {
    const NAME: &'static str = "combat.respawn";
    type Host = UnitEntity;

    // Its decode keeps its tick within the limit.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for Respawn {
    const KIND: DataKind = DataKind::Prediction;
    type Sending = SentPredicted;
}
