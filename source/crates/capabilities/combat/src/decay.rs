use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_sim::{DueIndex, SimComponent, TimedKept, TimedProcess, TimedRuns};
use campfire_states::{Replication, SentOnInsert};
use campfire_units::UnitEntity;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

/// When a dead unit its type keeps despawns: at the end of tick `at`, unless a later stage keeps
/// it still (04-capabilities/combat.md, A death keeps its unit for its animation). It is inserted as the
/// unit dies, never changed in place, and its hook pushes its unit into the index of the decays
/// due at each insert, a restore's too. It reaches each client that holds its unit, which so
/// tells the end of a death from its unit leaving its sight (04-capabilities/combat.md, A death's
/// decay reaches its clients).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[component(immutable, on_insert = DueIndex::<Self>::pushed)]
pub struct Decay {
    pub at: Tick,
}

/// A decay comes due in its tick on the server, which despawns the units it holds.
impl TimedProcess for Decay {
    const RUNS: TimedRuns = TimedRuns::Server;
    const KEPT: TimedKept = TimedKept::BySetters;
    type Parts = &'static Self;
    type Books = ();

    fn due((): &(), decay: &Self) -> Option<Tick> {
        Some(decay.at)
    }
}

impl SimComponent for Decay {
    const NAME: &'static str = "combat.decay";
    type Host = UnitEntity;

    // Its decode keeps its tick within the limit.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for Decay {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentOnInsert;
}
