use bevy_ecs::component::Component;
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::{ScheduleBuildError, ScheduleBuildWarning, ScheduleConfigs};
use bevy_ecs::system::{Commands, Query, ScheduleSystem};
use campfire_math::{Num, RngSource};
use serde::{Deserialize, Serialize};

use super::*;
use crate::sim_state::{SimComponent, SimResource};
use crate::stable_id::StableId;
use crate::state_registry::{StateHash, StateRegistry};

const SEED: SegmentSeed = SegmentSeed::new([7; 32]);
const OTHER_SEED: SegmentSeed = SegmentSeed::new([8; 32]);
const TICKS: u64 = 5;
/// Each tick moves a unit by a draw below this many raw units.
const STEP_BOUND: u64 = 1000;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Position(Num);

impl SimComponent for Position {
    const NAME: &'static str = "test.position";
}

/// Ticks lived, counting the tick of the spawn.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Age(u64);

impl SimComponent for Age {
    const NAME: &'static str = "test.age";
}

/// The sum of the tick numbers seen so far.
#[derive(Resource, Debug, Default, Serialize, Deserialize)]
struct TickSum(u64);

impl SimResource for TickSum {
    const NAME: &'static str = "test.tick_sum";
}

fn spawn_unit(mut commands: Commands<'_, '_>, mut ids: ResMut<'_, IdAllocator>) {
    commands.spawn((ids.allocate(), Position(Num::ZERO), Age(0)));
}

fn wander(rng: Res<'_, SimRng>, mut units: Query<'_, '_, (&StableId, &mut Position)>) {
    for (&id, mut position) in &mut units {
        position.0 += Num::from_bits(rng.open("wander", id).below(STEP_BOUND).cast_signed());
    }
}

fn push(mut units: Query<'_, '_, &mut Position>) {
    for mut position in &mut units {
        position.0 += Num::ONE;
    }
}

fn grow_older(mut units: Query<'_, '_, &mut Age>) {
    for mut age in &mut units {
        age.0 += 1;
    }
}

fn sum_ticks(tick: Res<'_, SimTick>, mut sum: ResMut<'_, TickSum>) {
    sum.0 += tick.get();
}

fn new_world(seed: SegmentSeed) -> World {
    let mut world = World::new();
    SimUpdate::prepare(&mut world, seed);
    world.init_resource::<TickSum>();
    world
}

fn registry() -> StateRegistry {
    let mut registry = StateRegistry::new();
    registry.register_component::<Position>();
    registry.register_component::<Age>();
    registry.register_resource::<TickSum>();
    registry
}

/// A spawn, two independent systems in one step, and a reader of the tick; `reversed` adds the
/// same systems in the opposite order.
fn workload(reversed: bool) -> Schedule {
    let mut systems: [ScheduleConfigs<ScheduleSystem>; 4] = [
        spawn_unit.in_set(SimSet::Inputs),
        wander.in_set(SimSet::BeforeCollision),
        grow_older.in_set(SimSet::BeforeCollision),
        sum_ticks.in_set(SimSet::AfterCollision),
    ];
    if reversed {
        systems.reverse();
    }
    let mut schedule = SimUpdate::schedule();
    for system in systems {
        schedule.add_systems(system);
    }
    schedule
}

fn hashes(seed: SegmentSeed, reversed: bool) -> Vec<StateHash> {
    let registry = registry();
    let mut world = new_world(seed);
    let mut schedule = workload(reversed);
    (0..TICKS)
        .map(|_| {
            schedule.run(&mut world);
            registry.hash(&world)
        })
        .collect()
}

fn name_of(system: &ScheduleSystem) -> String {
    system.name().shortname().to_string()
}

/// The pairs of systems the build reports as conflicting, or none when it builds.
fn conflicts(add: fn(&mut Schedule)) -> Vec<[String; 2]> {
    let mut world = new_world(SEED);
    let mut schedule = SimUpdate::schedule();
    add(&mut schedule);
    match schedule.initialize(&mut world) {
        Ok(_) => Vec::new(),
        Err(ScheduleBuildError::Elevated(ScheduleBuildWarning::Ambiguity(warning))) => {
            let systems = &schedule.graph().systems;
            let name = |key| name_of(systems.get(key).unwrap().system());
            warning
                .0
                .iter()
                .map(|&(a, b, _)| {
                    let mut pair = [name(a), name(b)];
                    pair.sort_unstable();
                    pair
                })
                .collect()
        }
        Err(other) => panic!("unexpected build error: {other}"),
    }
}

#[derive(Debug)]
struct Case {
    name: &'static str,
    add: fn(&mut Schedule),
    expected: &'static [[&'static str; 2]],
}

#[test]
fn conflicting_unordered_systems_fail_the_build() {
    let cases = [
        Case {
            name: "two writers of one component in one step",
            add: |s| {
                s.add_systems((wander, push).in_set(SimSet::BeforeCollision));
            },
            expected: &[["push", "wander"]],
        },
        Case {
            name: "the same writers in order",
            add: |s| {
                s.add_systems((wander, push).chain().in_set(SimSet::BeforeCollision));
            },
            expected: &[],
        },
        Case {
            name: "the same writers in two steps",
            add: |s| {
                s.add_systems((
                    wander.in_set(SimSet::BeforeCollision),
                    push.in_set(SimSet::AfterCollision),
                ));
            },
            expected: &[],
        },
        Case {
            name: "an RNG reader outside the steps, unordered with the tick start",
            add: |s| {
                s.add_systems(wander);
            },
            expected: &[["start_tick", "wander"]],
        },
        Case {
            name: "a tick reader outside the steps, unordered with the tick end",
            add: |s| {
                s.add_systems(sum_ticks);
            },
            expected: &[["end_tick", "sum_ticks"]],
        },
        Case {
            name: "systems with disjoint access in one step",
            add: |s| {
                s.add_systems((wander, grow_older, sum_ticks).in_set(SimSet::Mode));
            },
            expected: &[],
        },
    ];
    for case in cases {
        assert_eq!(conflicts(case.add), case.expected, "{}", case.name);
    }
}

#[test]
fn same_seed_gives_same_hash_every_tick() {
    let forward = hashes(SEED, false);
    assert_eq!(hashes(SEED, false), forward);

    // Adding the systems in the opposite order runs the independent pair in the opposite order,
    // with the same result.
    let order = |reversed: bool| {
        let mut schedule = workload(reversed);
        schedule.initialize(&mut new_world(SEED)).unwrap();
        schedule
            .systems()
            .unwrap()
            .map(|(_, system)| name_of(system))
            .filter(|name| name == "wander" || name == "grow_older")
            .collect::<Vec<_>>()
    };
    assert_ne!(order(false), order(true));
    assert_eq!(hashes(SEED, true), forward);

    let mut distinct = forward.clone();
    distinct.sort_by_key(|hash| *hash.as_bytes());
    distinct.dedup();
    assert_eq!(
        distinct.len(),
        forward.len(),
        "every tick changes the state"
    );
    for (tick, (other, same)) in hashes(OTHER_SEED, false).iter().zip(&forward).enumerate() {
        assert_ne!(other, same, "tick {tick}: the seed keys the draws");
    }

    // A world restored after tick 2 goes on with the same hashes: the tick is state.
    let registry = registry();
    let mut world = new_world(SEED);
    let mut schedule = workload(false);
    for _ in 0..3 {
        schedule.run(&mut world);
    }
    let mut snapshot = Vec::new();
    registry.snapshot(&world, &mut snapshot);
    let mut restored = new_world(SEED);
    registry.restore(&snapshot, &mut restored).unwrap();
    let mut schedule = workload(false);
    for expected in &forward[3..] {
        schedule.run(&mut restored);
        assert_eq!(registry.hash(&restored), *expected);
    }
}

#[test]
fn ticks_advance_and_key_the_draws() {
    let mut world = new_world(SEED);
    let mut schedule = workload(false);
    for _ in 0..TICKS {
        schedule.run(&mut world);
    }
    assert_eq!(world.resource::<SimTick>().get(), TICKS);
    // Ticks 0 to 4 each add their number: 0 + 1 + 2 + 3 + 4 = 10.
    assert_eq!(world.resource::<TickSum>().0, 10);

    // Unit k spawns in tick k and moves in the same tick, so it moves in ticks k to 4, by the draw
    // of stream "wander", entity k, tick t each time.
    let reference = |id: u64| {
        let mut source = RngSource::new(SEED);
        (id..TICKS)
            .map(|tick| {
                source.begin_tick(tick);
                source.open("wander", id).below(STEP_BOUND)
            })
            .sum::<u64>()
    };
    let units: Vec<_> = world
        .resource::<EntityIndex>()
        .iter()
        .map(|(id, entity)| {
            let unit = world.entity(entity);
            (
                id.get(),
                unit.get::<Position>().unwrap().0.to_bits(),
                unit.get::<Age>().unwrap().0,
            )
        })
        .collect();
    let expected: Vec<_> = (0..TICKS)
        .map(|id| (id, reference(id).cast_signed(), TICKS - id))
        .collect();
    assert_eq!(units, expected);
}
