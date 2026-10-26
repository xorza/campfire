use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::Remove;
use bevy_ecs::observer::On;
use bevy_ecs::query::Has;
use bevy_ecs::system::{Query, ResMut};
use bevy_ecs::world::World;
use campfire_common::{Tick, Ticks};
use campfire_sim::{DueIndex, SimComponent, StableId, TimedKept, TimedProcess, TimedRuns};
use campfire_states::{NotSent, Replication};
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::dead::Dead;
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

    /// Pushes the unit whose `Dead` goes into the index of the lives that end, at its last tick,
    /// even one past, as it lives again: an observer of `Dead`'s removal, which runs while the
    /// unit is still dead, so `due` would read no time.
    pub(crate) fn revived(
        removed: On<'_, '_, Remove<Dead>>,
        lives: Query<'_, '_, (&StableId, &Self)>,
        index: Option<ResMut<'_, DueIndex<Self>>>,
    ) {
        let entity = removed.entity;
        if let (Ok((&id, life)), Some(mut index)) = (lives.get(entity), index) {
            index.push(life.last_tick(), id, entity);
        }
    }
}

/// A timed life comes due in its last tick, which ends it; a dead unit's life waits, as its
/// death owns it until it despawns or lives again (04-capabilities/combat.md, A death keeps its
/// unit for its animation).
impl TimedProcess for Lifespan {
    const RUNS: TimedRuns = TimedRuns::Server;
    const KEPT: TimedKept = TimedKept::BySetters;
    type Parts = (&'static Self, Has<Dead>);
    type Books = ();

    fn due((): &(), (life, dead): (&Self, bool)) -> Option<Tick> {
        (!dead).then(|| life.last_tick())
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
