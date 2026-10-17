//! The benches of a whole match, which only the top crate builds.

use std::collections::BTreeMap;
use std::num::NonZeroU32;

use bevy_ecs::bundle::Bundle;
use bevy_ecs::world::World;
use campfire_actions::{ActionSlots, Actions, GatherSpec, SlotKind, TimedSlots};
use campfire_common::{PlayerSlot, SegmentSeed, Ticks};
use campfire_declare::{Book, Role};
use campfire_math::{Num, Vec3};
use campfire_players::{Players, PlayersWorld};
use campfire_production::{GatherOrder, GatherStep, Gatherer, Node, NodeAt, NodeBook, ResourceSet};
use campfire_script::Filter;
use campfire_sim::{Capability, IdAllocator, SimUpdate, StableId, TickRate};
use campfire_units::{
    Body, BodyForm, BodyUnit, Destination, Owner, TypeOrigin, UnitCore, UnitTypeCore, UnitTypes,
};
use campfire_values::{PackageIndex, Rank, RelationSet, ResourceId, Team, TypeScope, UnitType};
use criterion::{Criterion, Throughput};

use crate::release::Release;
use crate::release::r#match::Match;
use crate::release::match_kind::MatchKind;
use crate::release::match_spec::MatchSpec;

/// Workers in each of the two fields.
const WORKERS: i64 = 100;
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
    let mut group = c.benchmark_group("integration/gather");
    group.throughput(Throughput::Elements(2 * WORKERS.unsigned_abs()));
    group.bench_function("two_fields", |bench| {
        let world = fields.get_or_insert_with(two_fields);
        bench.iter(|| world.run_schedule(SimUpdate));
    });
    group.finish();
}

/// The two fields of `gather`, past the 30 ticks that bring the loop to its steady state.
fn two_fields() -> Match {
    let rate = TickRate::new(NonZeroU32::new(30).unwrap());
    // A match without navigation runs no gather, as only navigation tests a box for room.
    let set = Release::capability_set(&[Capability::Production, Capability::Navigation]).unwrap();
    let spec = MatchSpec::new(SegmentSeed::ZERO, rate, Role::Rules).set(set);
    let mut world = Match::new(spec, Release::empty_books(rate), MatchKind::Rules);

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
    book.nodes.set(node_type, gold);
    book.drop_offs.set(drop_off_type, ResourceSet::from([gold]));
    world.insert_resource(book);
    let spec = GatherSpec {
        resource: gold,
        take: NonZeroU32::new(5).unwrap(),
        bounce: Num::int(4),
    };
    let all = Filter::of_match_relations(&world, RelationSet::All);
    let action = Actions::add_gather(&mut world, spec, all, Num::int(16), Ticks::new(3));

    let player = Owner::new(PlayerSlot::new(0));
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
                let pos = at(Num::int(field - 7 + 2 * nth), Num::int(3));
                let body = BodyForm::box_sized([Num::int(2), Num::ONE]).unwrap();
                let core = Core {
                    unit_type: node_type,
                    team: Team::new(2),
                    owner: None,
                };
                let parts = (body.at(Num::ZERO), Node::new(u32::MAX));
                let node = spawn(&mut world, core, pos, parts);
                NodeAt { node, pos }
            })
            .collect();
        for nth in 0..WORKERS {
            // Spread over x −8 to 8 of the field, before the node in front.
            let x = Num::int(16 * nth + 8) / Num::int(WORKERS) - Num::int(8);
            let node = nodes[usize::try_from(nth * NODES / WORKERS).unwrap()];
            let mut gatherer = Gatherer::default();
            gatherer.set(Some(GatherOrder {
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
        world.run_schedule(SimUpdate);
    }
    let gold_in = world
        .player_resources(PlayerSlot::new(0))
        .unwrap()
        .amount(gold);
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

/// A unit of `core` at `pos` with `parts`, of the next stable id, spawned as every unit is.
fn spawn(world: &mut World, core: Core, pos: Vec3, parts: impl Bundle) -> StableId {
    let Core {
        unit_type,
        team,
        owner,
    } = core;
    let id = world.resource_mut::<IdAllocator>().allocate();
    let core = UnitCore {
        id,
        pos,
        unit_type,
        team,
        owner,
    };
    core.spawn(world, BodyUnit, parts);
    id
}

/// Runs each bench of the crate whose id criterion's filter takes.
pub fn run(c: &mut Criterion) {
    gather(c);
}
