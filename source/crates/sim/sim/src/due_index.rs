use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::marker::PhantomData;

use bevy_ecs::entity::Entity;
use bevy_ecs::observer::On;
use bevy_ecs::query::Allow;
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::system::{Query, Res, ResMut, StaticSystemParam};
use bevy_ecs::world::World;
use campfire_common::Tick;

use crate::prediction_started::PredictionStarted;
use crate::sim_update::{TickEnd, end_tick};
use crate::stable_id::StableId;
use crate::state_rewritten::StateRewritten;
use crate::timed_process::{TimedKept, TimedProcess};
use crate::unpredicted::Unpredicted;

/// The units a timed process `P` visits when their time comes, each once, at the earliest tick
/// pushed for it since its last visit: a process pushes a unit where it sets the unit's time, and
/// its pass takes only the units due, by tick, then stable id, so the order of its visits is the
/// same in every world that holds the same state (04-capabilities/00-overview.md, One due index
/// for each timed process). An entry is a visit, never a promise: one whose unit's time moved
/// later, or that went, costs a visit that finds nothing due, and the pass pushes the unit at its
/// new time. A cache, which builds again from the parts as the state is written from outside the
/// sim; not state.
#[derive(Resource)]
pub struct DueIndex<P> {
    /// Each unit due, by tick, then stable id.
    order: BTreeSet<(Tick, StableId)>,
    /// The tick each unit is due at, and its entity, by stable id.
    held: BTreeMap<StableId, Held>,
    process: PhantomData<fn() -> P>,
}

/// A unit due: when, and its entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Held {
    at: Tick,
    entity: Entity,
}

/// A unit a pass visits as its time came.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DueUnit {
    pub id: StableId,
    pub entity: Entity,
}

impl<P> DueIndex<P> {
    /// Visits the unit `id`, of `entity`, at `at`, or at the tick it is due at when that comes
    /// first.
    pub fn push(&mut self, at: Tick, id: StableId, entity: Entity) {
        match self.held.get(&id) {
            Some(held) if held.at <= at && held.entity == entity => return,
            Some(held) => {
                self.order.remove(&(held.at, id));
            }
            None => {}
        }
        self.held.insert(id, Held { at, entity });
        self.order.insert((at, id));
    }

    /// Moves each unit due at `now` or before into `due`, out of the index, by tick, then stable
    /// id.
    pub fn take_due(&mut self, now: Tick, due: &mut Vec<DueUnit>) {
        while let Some(&(at, id)) = self.order.first()
            && at <= now
        {
            self.order.pop_first();
            let held = self.held.remove(&id).expect("each unit due is held");
            due.push(DueUnit {
                id,
                entity: held.entity,
            });
        }
    }

    /// Moves each unit due at `now` or before that `takes` takes into `due`, out of the index, by
    /// tick, then stable id; the others stay, for another pass of the same index.
    pub fn take_due_if(
        &mut self,
        now: Tick,
        mut takes: impl FnMut(DueUnit) -> bool,
        due: &mut Vec<DueUnit>,
    ) {
        let from = due.len();
        due.extend(self.peek_due(now).filter(|&unit| takes(unit)));
        for unit in &due[from..] {
            let held = self.held.remove(&unit.id).expect("each unit due is held");
            self.order.remove(&(held.at, unit.id));
        }
    }

    /// Takes the unit `id` out of the index.
    pub fn forget(&mut self, id: StableId) {
        if let Some(held) = self.held.remove(&id) {
            self.order.remove(&(held.at, id));
        }
    }

    /// Each unit due at `now` or before, by tick, then stable id, which the index keeps.
    pub fn peek_due(&self, now: Tick) -> impl Iterator<Item = DueUnit> + '_ {
        let due = self.order.iter().take_while(move |&&(at, _)| at <= now);
        due.map(|&(_, id)| DueUnit {
            id,
            entity: self.held[&id].entity,
        })
    }

    /// The tick the unit `id` is due at, when the index holds it.
    pub fn due_at(&self, id: StableId) -> Option<Tick> {
        self.held.get(&id).map(|held| held.at)
    }

    /// How many units it holds.
    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// Forgets every unit.
    fn clear(&mut self) {
        self.order.clear();
        self.held.clear();
    }
}

impl<P: TimedProcess> DueIndex<P> {
    /// Adds what keeps the index of `P` true in `world`: the observers that build it again as
    /// the state is rewritten and push a unit a client starts to predict, and, in a debug build,
    /// the check at each tick's end that each unit due is held, in `schedule`.
    pub fn track(world: &mut World, schedule: &mut Schedule) {
        world.add_observer(Self::rebuild);
        world.add_observer(Self::predicted);
        if cfg!(debug_assertions) && P::KEPT == TimedKept::BySetters {
            schedule.add_systems(Self::audit.in_set(TickEnd).before(end_tick));
        }
    }

    /// Builds the index again from the parts of each unit the sim runs, as the state was written
    /// from outside the sim.
    fn rebuild(
        _: On<'_, '_, StateRewritten>,
        books: StaticSystemParam<'_, '_, P::Books>,
        units: Query<'_, '_, (Entity, &StableId, P::Parts)>,
        mut index: ResMut<'_, Self>,
    ) {
        index.clear();
        for (entity, &id, parts) in &units {
            if let Some(at) = P::due(&books, parts) {
                index.push(at, id, entity);
            }
        }
    }

    /// Pushes a unit a predicting client starts to predict, whose parts replication wrote.
    fn predicted(
        started: On<'_, '_, PredictionStarted>,
        books: StaticSystemParam<'_, '_, P::Books>,
        units: Query<'_, '_, (&StableId, P::Parts), Allow<Unpredicted>>,
        index: Option<ResMut<'_, Self>>,
    ) {
        let entity = started.entity;
        let (Some(mut index), Ok((&id, parts))) = (index, units.get(entity)) else {
            return;
        };
        if let Some(at) = P::due(&books, parts) {
            index.push(at, id, entity);
        }
    }

    /// Checks, as each tick ends and before it advances, that each unit the sim runs that has a
    /// time to come is held at that time or before: a pass that set a time and pushed none is a
    /// logic error.
    fn audit(
        books: StaticSystemParam<'_, '_, P::Books>,
        units: Query<'_, '_, (Entity, &StableId, P::Parts)>,
        index: Res<'_, Self>,
    ) {
        for (_, &id, parts) in &units {
            let Some(at) = P::due(&books, parts) else {
                continue;
            };
            let held = index.due_at(id);
            assert!(
                held.is_some_and(|held| held <= at),
                "{} is due at {at:?} but its index holds {held:?}: a pass set its time and pushed \
                 none",
                id.get()
            );
        }
    }
}

impl<P> Default for DueIndex<P> {
    fn default() -> Self {
        Self {
            order: BTreeSet::new(),
            held: BTreeMap::new(),
            process: PhantomData,
        }
    }
}

impl<P> fmt::Debug for DueIndex<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DueIndex")
            .field("order", &self.order)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::panic::{self, AssertUnwindSafe};

    use bevy_ecs::component::Component;
    use bevy_ecs::system::RunSystemOnce;

    use bevy_ecs::entity_disabling::DefaultQueryFilters;

    use super::*;
    use crate::stable_id::entity_index::EntityIndex;
    use crate::timed_process::TimedRuns;

    /// A process of the tests, whose units are due at their `Probe`'s tick.
    #[derive(Component, Debug, Clone, Copy)]
    struct Probe(Option<Tick>);

    impl TimedProcess for Probe {
        const RUNS: TimedRuns = TimedRuns::EveryRole;
        const KEPT: TimedKept = TimedKept::BySetters;
        type Parts = &'static Self;
        type Books = ();

        fn due((): &(), probe: &Self) -> Option<Tick> {
            probe.0
        }
    }

    #[test]
    fn the_index_builds_again_as_the_state_is_rewritten_and_the_audit_finds_a_missed_push() {
        let mut world = World::new();
        world.init_resource::<EntityIndex>();
        world.init_resource::<DefaultQueryFilters>();
        Unpredicted::register(&mut world);
        world.init_resource::<DueIndex<Probe>>();
        let mut schedule = Schedule::default();
        DueIndex::<Probe>::track(&mut world, &mut schedule);
        let [a, b, c] = [1, 2, 3].map(StableId::new);
        let due_a = world.spawn((a, Probe(Some(Tick::new(4))))).id();
        world.spawn((b, Probe(None)));
        let hidden = world
            .spawn((c, Probe(Some(Tick::new(2))), Unpredicted))
            .id();
        // The audit fails on a unit due that no push gave the index.
        let audit = |world: &mut World| {
            panic::catch_unwind(AssertUnwindSafe(|| {
                world.run_system_once(DueIndex::<Probe>::audit).unwrap();
            }))
            .is_ok()
        };
        assert!(!audit(&mut world));
        // A rewrite builds it from the units the sim runs: `a`, not `b` with no time, nor `c`,
        // hidden; then `c` is predicted again and pushed.
        world.trigger(StateRewritten);
        let index = world.resource::<DueIndex<Probe>>();
        assert_eq!(
            (index.len(), index.due_at(a), index.due_at(c)),
            (1, Some(Tick::new(4)), None)
        );
        world.entity_mut(hidden).remove::<Unpredicted>();
        world.trigger(PredictionStarted { entity: hidden });
        assert_eq!(
            world.resource::<DueIndex<Probe>>().due_at(c),
            Some(Tick::new(2))
        );
        assert!(audit(&mut world));
        // A time set earlier with no push is a missed push; the push mends it.
        world.entity_mut(due_a).insert(Probe(Some(Tick::new(3))));
        assert!(!audit(&mut world));
        world
            .resource_mut::<DueIndex<Probe>>()
            .push(Tick::new(3), a, due_a);
        assert!(audit(&mut world));
    }

    #[test]
    fn the_index_gives_each_unit_once_at_its_earliest_push_by_tick_then_stable_id() {
        let entity = |index| Entity::from_raw_u32(index).unwrap();
        let [a, b, c] = [3, 1, 2].map(StableId::new);
        let mut index = DueIndex::<Probe>::default();
        index.push(Tick::new(5), a, entity(10));
        index.push(Tick::new(5), b, entity(11));
        index.push(Tick::new(9), c, entity(12));
        // A push at a later tick adds nothing; one at an earlier tick moves the unit there.
        index.push(Tick::new(7), a, entity(10));
        index.push(Tick::new(4), c, entity(12));
        assert_eq!(index.len(), 3);
        assert_eq!(index.due_at(c), Some(Tick::new(4)));
        let mut due = Vec::new();
        index.take_due(Tick::new(3), &mut due);
        assert_eq!(due, []);
        // Tick 5 takes c, due at 4, then b and a at 5, b's lower id first: by tick, then id.
        index.take_due(Tick::new(5), &mut due);
        let ids: Vec<StableId> = due.iter().map(|unit| unit.id).collect();
        assert_eq!(ids, [c, b, a]);
        assert_eq!(due[0].entity, entity(12));
        assert!(index.is_empty());
        // Taken, a unit is pushed afresh, at any tick.
        due.clear();
        index.push(Tick::new(8), a, entity(10));
        index.take_due(Tick::new(8), &mut due);
        assert_eq!(
            due,
            [DueUnit {
                id: a,
                entity: entity(10)
            }]
        );
        // A take that leaves a unit keeps it for another pass, and a unit forgotten is gone.
        let mut index = DueIndex::<Probe>::default();
        index.push(Tick::new(2), a, entity(10));
        index.push(Tick::new(3), b, entity(11));
        index.push(Tick::new(4), c, entity(12));
        due.clear();
        index.take_due_if(Tick::new(3), |unit| unit.id != b, &mut due);
        let ids: Vec<StableId> = due.iter().map(|unit| unit.id).collect();
        assert_eq!(ids, [a]);
        assert_eq!((index.due_at(b), index.len()), (Some(Tick::new(3)), 2));
        index.forget(b);
        index.forget(b);
        assert_eq!(
            (index.due_at(b), index.due_at(c), index.len()),
            (None, Some(Tick::new(4)), 1)
        );
        // A unit whose entity changed, as a restore's, takes the new one at the new tick.
        let mut index = DueIndex::<Probe>::default();
        index.push(Tick::new(2), a, entity(10));
        index.push(Tick::new(6), a, entity(20));
        due.clear();
        index.take_due(Tick::new(6), &mut due);
        assert_eq!(
            due,
            [DueUnit {
                id: a,
                entity: entity(20)
            }]
        );
    }
}
