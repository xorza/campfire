use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::{Tick, Ticks};
use campfire_sim::{DueIndex, SimComponent, TimedKept, TimedProcess, TimedRuns};
use campfire_states::{NotSent, Replication};
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::unit_entity::UnitEntity;

/// A timed life, as a ward's: the first tick the unit no longer lives, at the end of the tick
/// before which it despawns. It never changes, and its hook pushes its unit into the index of the
/// lives that end, at each insert, a spawn's and a restore's alike. State of the core.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[component(immutable, on_insert = DueIndex::<Self>::pushed)]
#[serde(transparent)]
pub struct Lifespan {
    ends: Tick,
}

impl Lifespan {
    pub const fn until(ends: Tick) -> Self {
        Self { ends }
    }

    pub const fn ends(self) -> Tick {
        self.ends
    }

    /// Whether its life ends with the tick that starts at `now`.
    pub(crate) fn ends_after(self, now: Tick) -> bool {
        self.ends <= now.after(Ticks::new(1))
    }

    /// The last tick in whose end its life ends: the one before `ends`.
    pub(crate) const fn last_tick(self) -> Tick {
        Tick::new(self.ends.get().saturating_sub(1))
    }
}

/// A timed life comes due in its last tick, which ends it.
impl TimedProcess for Lifespan {
    const RUNS: TimedRuns = TimedRuns::Server;
    const KEPT: TimedKept = TimedKept::BySetters;
    type Parts = &'static Self;
    type Books = ();

    fn due((): &(), life: &Self) -> Option<Tick> {
        Some(life.last_tick())
    }
}

impl SimComponent for Lifespan {
    const NAME: &'static str = "units.lifespan";
    type Host = UnitEntity;

    // Its decode keeps its tick within the limit.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for Lifespan {
    const KIND: DataKind = DataKind::Server;
    type Sending = NotSent;
}
