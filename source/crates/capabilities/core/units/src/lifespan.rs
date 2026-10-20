use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::world::{DeferredWorld, World};
use campfire_common::{Tick, Ticks};
use campfire_sim::{DueIndex, SimComponent, StableId, TimedKept, TimedProcess, TimedRuns};
use campfire_state_types::{NotSent, Replication};
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::unit_entity::UnitEntity;

/// A timed life, as a ward's: the first tick the unit no longer lives, at the end of the tick
/// before which it despawns. It never changes, and its hook pushes its unit into the index of the
/// lives that end, at each insert, a spawn's and a restore's alike. State of the core.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[component(immutable, on_insert = Lifespan::pushed)]
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

    /// Pushes the unit whose life it is into the index of the lives that end, at its last tick,
    /// in a match that ends lives.
    fn pushed(mut world: DeferredWorld<'_>, context: HookContext) {
        let entity = context.entity;
        let (Some(&life), Some(&id)) = (world.get::<Self>(entity), world.get::<StableId>(entity))
        else {
            return;
        };
        if let Some(mut index) = world.get_resource_mut::<DueIndex<Self>>() {
            index.push(life.last_tick(), id, entity);
        }
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
