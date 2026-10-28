use std::collections::BTreeMap;
use std::panic::{self, AssertUnwindSafe};

use bevy_ecs::component::Component;
use bevy_ecs::system::RunSystemOnce;

use bevy_ecs::entity_disabling::DefaultQueryFilters;
use campfire_common::Ticks;

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
fn the_index_builds_again_as_the_state_is_rewritten_holds_a_unit_back_in_the_world_and_the_audit_finds_a_missed_push()
 {
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
        (index.len(), index.due_at(due_a), index.due_at(hidden)),
        (1, Some(Tick::new(4)), None)
    );
    world.entity_mut(hidden).remove::<Unpredicted>();
    world.trigger(PredictionStarted { entity: hidden });
    assert_eq!(
        world.resource::<DueIndex<Probe>>().due_at(hidden),
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
    // A unit out of the world is held by no rewrite; it is pushed as it comes back.
    let out = (StableId::new(4), Probe(Some(Tick::new(6))), Disabled);
    let out = world.spawn(out).id();
    world.trigger(StateRewritten);
    assert_eq!(world.resource::<DueIndex<Probe>>().due_at(out), None);
    world.entity_mut(out).remove::<Disabled>();
    assert_eq!(
        world.resource::<DueIndex<Probe>>().due_at(out),
        Some(Tick::new(6))
    );
}

/// A process whose part's insert hook pushes its unit, as each such process's does.
#[derive(Component, Debug, Clone, Copy)]
#[component(on_insert = DueIndex::<Hooked>::pushed)]
struct Hooked(Option<Tick>);

impl TimedProcess for Hooked {
    const RUNS: TimedRuns = TimedRuns::EveryRole;
    const KEPT: TimedKept = TimedKept::BySetters;
    type Parts = &'static Self;
    type Books = ();

    fn due((): &(), hooked: &Self) -> Option<Tick> {
        hooked.0
    }
}

#[test]
fn a_part_that_comes_pushes_its_unit_at_its_time_in_a_match_that_runs_it() {
    let mut world = World::new();
    world.init_resource::<EntityIndex>();
    let [a, b] = [1, 2].map(StableId::new);
    // No index: a match that does not run the process pushes nothing.
    world.spawn((a, Hooked(Some(Tick::new(5)))));
    world.init_resource::<DueIndex<Hooked>>();
    let unit = world.spawn((b, Hooked(None))).id();
    assert!(world.resource::<DueIndex<Hooked>>().is_empty());
    // A unit with no time to come is not pushed; its next insert, of a time, pushes it.
    world.entity_mut(unit).insert(Hooked(Some(Tick::new(7))));
    assert_eq!(
        world.resource::<DueIndex<Hooked>>().due_at(unit),
        Some(Tick::new(7))
    );
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
    assert_eq!(index.due_at(entity(12)), Some(Tick::new(4)));
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
    assert_eq!(
        (index.due_at(entity(11)), index.len()),
        (Some(Tick::new(3)), 2)
    );
    index.forget(entity(11));
    index.forget(entity(11));
    assert_eq!(
        (
            index.due_at(entity(11)),
            index.due_at(entity(12)),
            index.len()
        ),
        (None, Some(Tick::new(4)), 1)
    );
    // An entity that takes the index of one gone takes its entry, at its own tick, earlier or
    // later; the gone one's forget leaves it.
    let gone = entity(10);
    let reused =
        Entity::from_index_and_generation(gone.index(), gone.generation().after_versions(1));
    let mut index = DueIndex::<Probe>::default();
    index.push(Tick::new(2), a, gone);
    index.push(Tick::new(6), b, reused);
    index.forget(gone);
    assert_eq!(
        (index.due_at(gone), index.due_at(reused)),
        (None, Some(Tick::new(6)))
    );
    due.clear();
    index.take_due(Tick::new(6), &mut due);
    assert_eq!(
        due,
        [DueUnit {
            id: b,
            entity: reused
        }]
    );
}

/// The heap against a model of sorted sets, the index's own form before it, over a run of pushes,
/// forgets, peeks and takes at random, on few enough entities that each is pushed again often.
#[test]
fn the_heap_gives_what_a_sorted_model_gives() {
    let mut model: BTreeMap<u32, (Tick, StableId)> = BTreeMap::new();
    let mut index = DueIndex::<Probe>::default();
    let entity = |slot| Entity::from_raw_u32(slot).unwrap();
    let ordered = |model: &BTreeMap<u32, (Tick, StableId)>, now: Tick| {
        let mut due: Vec<(Tick, StableId, u32)> = model
            .iter()
            .filter(|(_, (at, _))| *at <= now)
            .map(|(&slot, &(at, id))| (at, id, slot))
            .collect();
        due.sort_unstable();
        due.into_iter()
            .map(|(_, id, slot)| DueUnit {
                id,
                entity: entity(slot),
            })
            .collect::<Vec<_>>()
    };
    // A fixed LCG, so the run is the same each time.
    let mut state = 0x2545_f491_u64;
    let mut next = |below: u64| {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (state >> 33) % below
    };
    let (mut peeked, mut taken) = (Vec::new(), Vec::new());
    for step in 0..20_000 {
        let slot = u32::try_from(next(64)).unwrap();
        let now = Tick::new(step / 8);
        match next(8) {
            0..=3 => {
                let at = now.after(Ticks::new(next(16)));
                // An entity's id is its slot's, so two entities never share one.
                let id = StableId::new(u64::from(slot) + 1);
                index.push(at, id, entity(slot));
                let held = model.entry(slot).or_insert((at, id));
                held.0 = held.0.min(at);
            }
            4 => {
                index.forget(entity(slot));
                model.remove(&slot);
            }
            5 => {
                peeked.clear();
                index.peek_due(now, &mut peeked);
                assert_eq!(peeked, ordered(&model, now), "step {step}");
            }
            6 => {
                taken.clear();
                index.take_due(now, &mut taken);
                assert_eq!(taken, ordered(&model, now), "step {step}");
                model.retain(|_, (at, _)| *at > now);
            }
            _ => {
                // Takes the units of even slots, and keeps the others.
                taken.clear();
                index.take_due_if(now, |unit| unit.entity.index_u32() % 2 == 0, &mut taken);
                let expected: Vec<DueUnit> = ordered(&model, now)
                    .into_iter()
                    .filter(|unit| unit.entity.index_u32() % 2 == 0)
                    .collect();
                for unit in &expected {
                    model.remove(&unit.entity.index_u32());
                }
                assert_eq!(taken, expected, "step {step}");
            }
        }
        assert_eq!(index.len(), model.len());
        for (&slot, &(at, _)) in &model {
            assert_eq!(index.due_at(entity(slot)), Some(at));
        }
    }
}
