//! The benches of a whole match, which only the top crate builds.

use std::collections::BTreeMap;
use std::num::NonZeroU32;

use bevy_ecs::bundle::{Bundle, NoBundleEffect};
use bevy_ecs::world::World;
use campfire_actions::{ActionProgress, ActionSlots, SlotKind, TimedSlots};
use campfire_combat::Combat;
use campfire_combat::internals::Armed;
use campfire_common::{Seat, Ticks};
use campfire_declarations::Role;
use campfire_declarations::internals::BookLoad;
use campfire_math::{Num, Vec3};
use campfire_measure::{Kind, Scale};
use campfire_orders::Orders;
use campfire_players::{Players, PlayersWorld};
use campfire_production::{
    EmptyNode, GatherOrder, GatherSpec, GatherStep, Gatherer, Node, NodeAt, NodeBook, NodeSpec,
    Production, ResourceSet,
};
use campfire_script::Filter;
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{Capability, IdAllocator, SimWorld, StableId};
use campfire_units::internals::Spawn;
use campfire_units::{
    Body, BodyForm, BodyUnit, Destination, Owner, TypeOrigin, UnitCore, UnitTypeCore, UnitTypes,
};
use campfire_values::{PackageIndex, Rank, RelationSet, ResourceId, Team, TypeScope, UnitType};
use criterion::{Criterion, Throughput};

use crate::release::Release;
use crate::release::r#match::Match;

/// Workers in each of the two fields.
const WORKERS: i64 = 100;
/// Idle scanners in each of the two lines.
const SCANNERS: i64 = 256;
/// A scan each 250 ms: at 30 ticks a second, ⌈250 · 30 / 1000⌉ = 8 ticks.
const SCAN_TICKS: i64 = 8;
/// Nodes in each field.
const NODES: i64 = 8;

/// The gather loop of 200 workers on 16 nodes and two drop-offs, `two_fields`, a tick of
/// production's schedule as a match runs it, with no navigation. Each field is a drop-off of 16
/// by 2 m, a row of 8 nodes of 2 by 1 m 1.5 m off its long side, and 100 workers of a half meter
/// between them. A worker's gather reaches 16 m, every node and the drop-off of its field, so
/// none walks, not even to a node it moves to. A trip takes 5 in 3 ticks; with 12.5 workers to a
/// node, a node frees every 3 ticks to the first of its waiting workers, and the rest go on
/// waiting. The nodes never run out: the bench measures the loop's steady state.
fn gather(c: &mut Criterion) {
    let mut fields = None;
    let mut group = Kind::Integration.group(c, "gather", Scale::Sample);
    group.throughput(Throughput::Elements(2 * WORKERS.unsigned_abs()));
    group.bench_function("two_fields", |bench| {
        let world = fields.get_or_insert_with(two_fields);
        bench.iter(|| world.step());
    });
    group.finish();
}

/// The two fields of `gather`, past the 30 ticks that bring the loop to its steady state.
fn two_fields() -> Match {
    // A match without navigation runs no gather, as only navigation tests a box for room. It
    // installs at its first tick, after the scene's books load.
    let mut world = Match::builder(&[Capability::Production, Capability::Navigation])
        .role(Role::Rules)
        .build();

    let gold = ResourceId::new(0);
    Players::start_without_mode(&mut world, [Team::new(0)], 1);
    // The scene's types and their books, as a match loads them before any unit spawns.
    let mut types = UnitTypes::new(Release::engine_tags());
    let [node_type, drop_off_type, worker_type] = ["node", "drop_off", "worker"].map(|name| {
        let origin = TypeOrigin {
            package: PackageIndex::MODE,
            name: name.into(),
        };
        types
            .load(TypeScope::Mode, origin, &UnitTypeCore::default())
            .unwrap()
    });
    let books = types.finish(&BTreeMap::new());
    world.insert_resource(books.tags);
    world.insert_resource(books.states);
    books.types.load_into(&mut world);
    let mut book = NodeBook::default();
    let goes = NodeSpec {
        resource: gold,
        empty: EmptyNode::Goes,
    };
    book.nodes.set(node_type, goes);
    book.drop_offs.set(drop_off_type, ResourceSet::from([gold]));
    world.insert_resource(book);
    let take = NonZeroU32::new(5).unwrap();
    let spec = GatherSpec {
        resource: gold,
        take,
        unit: take,
        bounce: Num::int(4),
        deliver: Ticks::ZERO,
    };
    let all = Filter::of_match_relations(&world, RelationSet::All);
    let action = Production::add_gather(&mut world, spec, all, Num::int(16), Ticks::new(3));

    let player = Owner::new(Seat::new(0));
    let at = |x: Num, z: Num| Vec3::new(x, Num::ZERO, z);
    for field in [0, 40] {
        let body = BodyForm::box_sized([Num::int(16), Num::int(2)]).unwrap();
        let core = Core {
            unit_type: drop_off_type,
            team: Team::new(0),
            owner: Some(player),
        };
        spawn(
            &mut world,
            core,
            at(Num::int(field), Num::ZERO),
            body.at(Num::ZERO),
        );
        let nodes: Vec<NodeAt> = (0..NODES)
            .map(|nth| {
                let position = at(Num::int(field - 7 + 2 * nth), Num::int(3));
                let body = BodyForm::box_sized([Num::int(2), Num::ONE]).unwrap();
                let core = Core {
                    unit_type: node_type,
                    team: Team::new(2),
                    owner: None,
                };
                let parts = (body.at(Num::ZERO), Node::new(u32::MAX));
                let node = spawn(&mut world, core, position, parts);
                NodeAt { node, position }
            })
            .collect();
        for nth in 0..WORKERS {
            // Spread over x −8 to 8 of the field, before the node in front.
            let x = Num::int(16 * nth + 8) / Num::int(WORKERS) - Num::int(8);
            let node = nodes[usize::try_from(nth * NODES / WORKERS).unwrap()];
            let mut gatherer = Gatherer::default();
            gatherer.set_unindexed(Some(GatherOrder {
                slot: 0,
                node,
                step: GatherStep::Ordered,
            }));
            let core = Core {
                unit_type: worker_type,
                team: Team::new(0),
                owner: Some(player),
            };
            let worker = (
                Body::new(Num::HALF).unwrap(),
                TimedSlots::new(ActionSlots::new([(
                    action,
                    SlotKind::new(0),
                    Some(Rank::FIRST),
                )])),
                gatherer,
                Destination::default(),
            );
            spawn(
                &mut world,
                core,
                at(Num::int(field) + x, Num::int(7) / Num::int(4)),
                worker,
            );
        }
    }
    // Tick 0 gives each node to its first worker by stable id, whose trip ends 3 ticks on and
    // frees it to the next: by tick 29, 9 trips of 5 at each of the 16 nodes.
    for _ in 0..30 {
        world.step();
    }
    let gold_in = world.player_resources(Seat::new(0)).unwrap().amount(gold);
    assert_eq!(gold_in, 2 * NODES * 9 * 5, "the scene runs the loop");
    world
}

/// A unit of the scene: its type, team and owner.
#[derive(Debug, Clone, Copy)]
struct Core {
    unit_type: UnitType,
    team: Team,
    owner: Option<Owner>,
}

/// A unit of `core` at `position` with `parts`, of the next stable id, spawned as every unit is.
fn spawn(
    world: &mut World,
    core: Core,
    position: Vec3,
    parts: impl Bundle<Effect: NoBundleEffect>,
) -> StableId {
    let Core {
        unit_type,
        team,
        owner,
    } = core;
    let id = world.resource_mut::<IdAllocator>().allocate();
    let core = UnitCore {
        id,
        position,
        unit_type,
        team,
        owner,
        angle: Num::ZERO,
    };
    core.spawn(world, BodyUnit, parts);
    id
}

/// The idle scans of 512 units of a weapon of 2 m, `idle`, a tick of the schedule as a match
/// runs it, with no navigation's walk: two lines of 256, a meter apart within a line and 4 m from
/// the other, so each scan visits the units near it and reaches none, and every unit stays idle,
/// the scan's worst case. Each scans every 8 ticks, staggered by stable id, so a tick scans 64.
fn acquire(c: &mut Criterion) {
    let mut lines = None;
    let mut group = Kind::Integration.group(c, "acquire", Scale::Sample);
    group.throughput(Throughput::Elements(
        (2 * SCANNERS / SCAN_TICKS).unsigned_abs(),
    ));
    group.bench_function("idle", |bench| {
        let world = lines.get_or_insert_with(two_lines);
        bench.iter(|| world.step());
    });
    group.finish();
}

/// The two lines of `acquire`, past the 8 ticks in which each unit scanned once.
fn two_lines() -> Match {
    let mut world = Match::builder(&[Capability::Combat, Capability::Orders])
        .role(Role::Rules)
        .build();
    Combat::load_weapon_stats(&mut world);
    let armed = Armed::melee(Num::int(100), Num::int(2), 1, 30, Num::int(10));
    let scanner = armed.load(&mut world);
    Orders::load_acquire(&mut world, scanner, 250, &["enemies"]);
    let mut ids = Vec::new();
    for (team, z) in [(0, 0), (1, 4)] {
        for nth in 0..SCANNERS {
            let at = Vec3::new(Num::int(nth), Num::ZERO, Num::int(z));
            ids.push(
                Spawn::of(scanner, at)
                    .team(Team::new(team))
                    .place(&mut world),
            );
        }
    }
    for _ in 0..SCAN_TICKS {
        world.step();
    }
    let idle = ids
        .iter()
        .all(|&id| world.part::<ActionProgress>(id).attack_target().is_none());
    assert!(idle, "the scene's units stay idle");
    world
}

/// Runs each bench of the crate whose id criterion's filter takes.
pub fn run(c: &mut Criterion) {
    gather(c);
    acquire(c);
}
