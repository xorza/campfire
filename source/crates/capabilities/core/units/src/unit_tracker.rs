use std::fmt;
use std::marker::PhantomData;

use bevy_ecs::change_detection::{CheckChangeTicks, DetectChanges, Ref, Tick};
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::Remove;
use bevy_ecs::observer::On;
use bevy_ecs::query::{Allow, Changed, Or, QueryFilter};
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Query, ResMut, SystemChangeTick, SystemParam};
use bevy_ecs::world::World;
use campfire_declare::Cache;
use campfire_math::Vec3;
use campfire_sim::{Position, StableId, Unpredicted};

use crate::body::Body;

/// A group of units a `UnitTracker` follows: each unit with a stable id and a place that `Filter`
/// selects, those a client only holds among them.
pub trait TrackedUnits: Send + Sync + 'static {
    type Filter: QueryFilter + 'static;
}

/// Whether the units of `T` came, went, moved or changed their bodies since a reader last took
/// them: by the change ticks of their stable ids, places and bodies, their count, and a mark for
/// what no change tick shows, which observers set: a unit that moves in or out of the group as a
/// part comes or goes, a body lost, and Bevy's check of change ticks. A mark too many costs one
/// take, never a stale reader. Derived from the units, never state.
#[derive(Resource, Debug)]
pub struct UnitTracker<T> {
    /// The last take, `None` before the first.
    taken: Option<Taken>,
    /// The run of the last reader that found the units as taken, which a later reader of the
    /// same run finds so too.
    checked: Option<Tick>,
    marked: bool,
    units: PhantomData<T>,
}

/// A take: the run it was made in, and how many units it took, so a unit gone shows.
#[derive(Debug, Clone, Copy)]
struct Taken {
    tick: Tick,
    units: usize,
}

/// A unit of the group as a reader takes it.
#[derive(Debug, Clone, Copy)]
pub struct TrackedUnit<'a> {
    pub entity: Entity,
    pub id: StableId,
    pub position: Vec3,
    pub body: Option<&'a Body>,
}

/// The units of `T`, and their tracker, as a reader sees them.
#[derive(SystemParam)]
pub struct Tracking<'w, 's, T>
where
    T: TrackedUnits,
{
    tracker: ResMut<'w, UnitTracker<T>>,
    units: Query<
        'w,
        's,
        (
            Entity,
            Ref<'static, StableId>,
            Ref<'static, Position>,
            Option<Ref<'static, Body>>,
        ),
        (<T as TrackedUnits>::Filter, Allow<Unpredicted>),
    >,
    /// The units whose stable id, place or body changed since this reader's last run, among
    /// them every one that changed since the last take, which a reader of another run took.
    changed: Query<
        'w,
        's,
        (
            Ref<'static, StableId>,
            Ref<'static, Position>,
            Option<Ref<'static, Body>>,
        ),
        (
            <T as TrackedUnits>::Filter,
            Allow<Unpredicted>,
            Or<(Changed<StableId>, Changed<Position>, Changed<Body>)>,
        ),
    >,
    ticks: SystemChangeTick,
}

impl<T> Default for UnitTracker<T> {
    fn default() -> Self {
        Self {
            taken: None,
            checked: None,
            marked: false,
            units: PhantomData,
        }
    }
}

impl<T: TrackedUnits> Cache for UnitTracker<T> {
    fn installed(_: &World) -> Option<Self> {
        Some(Self::default())
    }
}

impl<T: TrackedUnits> UnitTracker<T> {
    /// Marks the units changed, as a reader must take them again.
    pub const fn mark(&mut self) {
        self.marked = true;
    }

    /// The observer of Bevy's check of change ticks, which clamps old ones and wraps none: a take
    /// or a check older than the maximum change age reads no change as newer, and a run's tick
    /// may come again.
    pub fn aged(_: On<'_, '_, CheckChangeTicks>, mut tracker: ResMut<'_, Self>) {
        tracker.marked = true;
        tracker.checked = None;
    }

    /// The observer of a body lost, which no change tick shows: a unit of the group that holds
    /// its body as a part it may lack.
    pub fn lost_body(_: On<'_, '_, Remove<Body>>, mut tracker: ResMut<'_, Self>) {
        tracker.marked = true;
    }
}

impl<T: TrackedUnits + fmt::Debug> fmt::Debug for Tracking<'_, '_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tracking")
            .field("tracker", &*self.tracker)
            .finish_non_exhaustive()
    }
}

impl<T: TrackedUnits> Tracking<'_, '_, T> {
    /// Whether the units changed since the last take, as a reader in this run sees them: the
    /// first reader of a run checks every unit's change ticks, and a later one of the same run
    /// finds them as taken. A reader that finds them changed takes them.
    pub fn is_stale(&mut self) -> bool {
        let this_run = self.ticks.this_run();
        if self.tracker.checked == Some(this_run) {
            return false;
        }
        let stale = self.is_changed(this_run);
        if !stale {
            self.tracker.checked = Some(this_run);
        }
        stale
    }

    /// Whether the units differ from the last take, as a reader in the run of `this_run` sees
    /// them: a unit changed since the take, or another count of them. It visits the units that
    /// changed since its own last run alone, which hold every change since the take, as each run
    /// of each reader came before the change, or saw it and took the units, or found them as
    /// taken.
    fn is_changed(&self, this_run: Tick) -> bool {
        let Some(taken) = self.tracker.taken.filter(|_| !self.tracker.marked) else {
            return true;
        };
        // A run's tick is the world's before it, so a change stamped with the take's tick came
        // before the take, and its reader's deferred commands stamp a later one.
        let newer = |tick: Tick| tick.is_newer_than(taken.tick, this_run);
        let moved = self.changed.iter().any(|(id, position, body)| {
            body.is_some_and(|body| newer(body.last_changed()))
                || newer(id.last_changed())
                || newer(position.last_changed())
        });
        // An archetypal filter counts by the archetypes' lengths, not unit by unit.
        moved || self.units.count() != taken.units
    }

    /// The units as they stand, for a reader that takes them.
    pub fn units(&self) -> impl Iterator<Item = TrackedUnit<'_>> {
        self.units
            .iter()
            .map(|(entity, id, position, body)| TrackedUnit {
                entity,
                id: *id,
                position: position.get(),
                body: body.map(Ref::into_inner),
            })
    }

    /// Notes the take of `units`, every unit as it stands, in this run.
    pub fn took(&mut self, units: usize) {
        debug_assert_eq!(units, self.units.iter().count(), "a take takes every unit");
        let this_run = self.ticks.this_run();
        self.tracker.taken = Some(Taken {
            tick: this_run,
            units,
        });
        self.tracker.marked = false;
        self.tracker.checked = Some(this_run);
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::change_detection::Tick;

    use crate::unit_tracker::{TrackedUnits, Tracking};

    impl<T: TrackedUnits> Tracking<'_, '_, T> {
        /// The run of the last take.
        pub fn taken_at(&self) -> Option<Tick> {
            Some(self.tracker.taken?.tick)
        }
    }
}
