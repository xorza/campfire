use std::num::NonZeroU32;

use campfire_sim::{EntityIndex, TickInput, TickInputs};

use super::*;
use crate::actions::gather_spec::GatherSpec;
use crate::geometry::bounds::Bounds;
use crate::geometry::grid::Grid;
use crate::navigation::Navigation;
use crate::navigation::walker::Walker;
use crate::orders::order::{Action, Order};
use crate::production::gatherer::{GatherStep, Gatherer, Load};
use crate::production::node::Node;
use crate::production::node_book::NodeBook;
use crate::production::rally::Rally;
use crate::production::rally_target::RallyTarget;
use crate::production::resource_set::ResourceSet;
use crate::units::block::Block;
use crate::units::body::{Body, BodyForm};
use crate::units::filter::Filter;
use crate::units::layer::Layer;
use crate::units::move_step::MoveStep;
use crate::units::unit_tags::UnitTags;
use crate::values::relation_set::RelationSet;

/// The point at `x` and `z` half meters.
fn half(x: i64, z: i64) -> Position {
    Position::new(Vec3::new(Num::HALF * x, Num::ZERO, Num::HALF * z)).unwrap()
}

/// A match of production and its orders over 1 m cells from (−20, −20) to (20, 20), for walkers
/// of a half meter. Its minerals are nodes of gold, boxes of 2 by 1 m of the neutral team 2; its
/// hall, player 0's, a box of 4 by 2 m, takes gold. A worker's gather takes 5 a trip in 3
/// ticks, within 1 m of its body's edge, and looks 4 m from its node for another.
#[derive(Debug)]
struct Mine {
    shop: Shop,
    gather: ActionId,
    gold: ResourceId,
    mineral: UnitType,
}

impl Mine {
    fn new() -> Mine {
        let mut shop = Shop::ordering();
        let world = &mut shop.sim.world;
        let bounds = Bounds::new([Num::int(-20); 2], [Num::int(20); 2]).unwrap();
        world.insert_resource(bounds);
        let walker = Walker {
            layer: Layer::FIRST,
            radius: Num::HALF,
        };
        let grid = Grid::new(Num::ONE, bounds).unwrap();
        Navigation::load_pathing(world, grid, &[], vec![walker]);
        let gold = ResourceId::named(&[DeclaredName::new("gold").unwrap()], "gold").unwrap();
        world.insert_resource(PlayerResources::new(1, 1));
        let mineral = Units::load_type(world, TypeScope::Mode, "mineral", &UnitTypeData::default());
        let mut nodes = NodeBook::default();
        nodes.nodes.set(mineral, gold);
        nodes.drop_offs.set(shop.depot, ResourceSet::of([gold]));
        world.insert_resource(nodes);
        let spec = GatherSpec {
            resource: gold,
            take: NonZeroU32::new(5).unwrap(),
            bounce: Num::int(4),
        };
        let all = Filter::of_relations(RelationSet::All);
        let gather = internals::gather(world, spec, all, Num::ONE, Ticks::new(3));
        Mine {
            shop,
            gather,
            gold,
            mineral,
        }
    }

    /// A worker of player `owner` at `at`, of a half meter, walking a meter a tick, with the
    /// gather in its slot 0.
    fn worker(&mut self, at: Position, owner: u32) -> StableId {
        let slots = ActionSlots::new([(self.gather, SlotKind::new(0), Rank::new(1))]);
        let parts = (
            Team::new(0),
            Owner::new(PlayerSlot::new(owner)),
            Body::new(Num::HALF).unwrap(),
            Navigation::walker(MoveStep::new(Num::ONE).unwrap()),
            slots,
            Gatherer::default(),
        );
        self.shop.sim.spawn(at, parts)
    }

    /// A mineral at `at` holding `amount`.
    fn mineral(&mut self, at: Position, amount: u32) -> StableId {
        let body = BodyForm::box_sized([Num::int(2), Num::ONE])
            .unwrap()
            .at(Num::ZERO);
        let parts = (self.mineral, Team::new(2), body, Node::new(amount));
        self.shop.sim.spawn(at, parts)
    }

    /// Player 0's hall at `at`.
    fn hall(&mut self, at: Position) -> StableId {
        let body = BodyForm::box_sized([Num::int(4), Num::int(2)])
            .unwrap()
            .at(Num::ZERO);
        let parts = (
            self.shop.depot,
            Team::new(0),
            Owner::new(PlayerSlot::new(0)),
            body,
        );
        self.shop.sim.spawn(at, parts)
    }

    /// Runs a tick in which player 0 orders each of `workers` to gather at `node`.
    fn gather_at(&mut self, workers: &[StableId], node: StableId) {
        let orders: Vec<Order> = workers
            .iter()
            .map(|&unit| {
                let target = ActionTarget::Unit(node);
                Order::one(unit, Action::Slot { slot: 0, target })
            })
            .collect();
        let payload = Order::payload(&orders);
        self.shop
            .sim
            .world
            .resource_mut::<TickInputs>()
            .push(TickInput {
                slot: PlayerSlot::new(0),
                payload: &payload,
            });
        self.shop.sim.step();
    }

    fn gold(&self) -> i64 {
        self.shop
            .sim
            .world
            .resource::<PlayerResources>()
            .amount(PlayerSlot::new(0), self.gold)
    }

    /// What `node` holds, none once gone.
    fn amount(&self, node: StableId) -> Option<u32> {
        self.shop
            .sim
            .try_get::<Node>(node)
            .map(|node| node.amount())
    }

    fn step(&self, worker: StableId) -> Option<GatherStep> {
        let gatherer = self.shop.sim.get::<Gatherer>(worker);
        gatherer.order().map(|order| order.step)
    }
}

#[test]
fn a_worker_brings_its_loads_in_trip_by_trip_and_stops_as_its_node_runs_out() {
    // The hall spans z −1 to 1 at the origin, a mineral of 12 z 5.5 to 6.5 at (0, 6); a worker
    // at (0.5, 3) walks the line x = 0.5 a meter a tick. It reaches the mineral from z = 4,
    // its edge 1 m off, the hall from z = 2. Tick 0 walks it to 4; tick 1 takes the mineral and
    // gathers 3 ticks, ending in tick 4, which carries 5 and walks to 3; tick 5 to 2; tick 6
    // delivers, 5 gold, and walks back to 3; tick 7 to 4, tick 8 takes the mineral again: a
    // trip of 7 ticks. Tick 13 delivers 10; tick 18 takes the last 2, and the mineral despawns
    // as the tick ends; tick 20 delivers 12, and the worker, with no mineral within 4 m of its
    // mineral's place, stops.
    let mut mine = Mine::new();
    mine.hall(half(0, 0));
    let node = mine.mineral(half(0, 12), 12);
    let worker = mine.worker(half(1, 6), 0);
    mine.gather_at(&[worker], node);
    let mut seen = Vec::new();
    for _ in 1..=21 {
        mine.shop.tick();
        seen.push((mine.gold(), mine.amount(node)));
    }
    let at = |tick: usize| seen[tick - 1];
    assert_eq!(at(1), (0, Some(12)));
    assert_eq!([at(4), at(5)], [(0, Some(7)), (0, Some(7))]);
    assert_eq!([at(6), at(10)], [(5, Some(7)), (5, Some(7))]);
    assert_eq!([at(11), at(13)], [(5, Some(2)), (10, Some(2))]);
    assert_eq!([at(17), at(18)], [(10, Some(2)), (10, None)]);
    assert_eq!([at(19), at(20)], [(10, None), (12, None)]);
    assert_eq!(mine.step(worker), None);
    assert_eq!(mine.shop.sim.get::<Gatherer>(worker).load(), None);
}

impl Mine {
    /// The node `worker`'s loop is at.
    fn node_of(&self, worker: StableId) -> Option<StableId> {
        let gatherer = self.shop.sim.get::<Gatherer>(worker);
        gatherer.order().map(|order| order.node.node)
    }

    /// The worker that holds `node`.
    fn holder(&self, node: StableId) -> Option<StableId> {
        self.shop.sim.get::<Node>(node).holder()
    }
}

#[test]
fn waiting_workers_take_a_freed_node_by_the_tick_they_waited_then_by_stable_id() {
    // A mineral at (0, 6), a hall at (0, 1.5), from z 0.5 to 2.5, and workers at x −1, 0 and 1
    // on z = 4, each in range of both: they never walk. In tick 0 the first takes the mineral,
    // the others wait from 0. Each gather ends 3 ticks on, delivers at once, and the worker
    // waits again from that tick: the node goes to the first, by its wait's tick, then by id.
    // The first frees it to the second in tick 3, the second to the third in tick 6, each of a
    // higher id than the one that frees it, and the third back to the first in tick 9, of a
    // lower id, which waited from tick 3, before the second, from 6.
    let mut mine = Mine::new();
    mine.hall(half(0, 3));
    let node = mine.mineral(half(0, 12), 100);
    let workers = [-2, 0, 2].map(|x| mine.worker(half(x, 8), 0));
    mine.gather_at(&workers, node);
    let mut holders = vec![mine.holder(node)];
    let mut gold = vec![mine.gold()];
    for _ in 1..=12 {
        mine.shop.tick();
        holders.push(mine.holder(node));
        gold.push(mine.gold());
    }
    let [first, second, third] = workers.map(Some);
    let by_tick = [
        first, first, first, second, second, second, third, third, third, first,
    ];
    assert_eq!(holders[..10], by_tick);
    assert_eq!(holders[12], second);
    assert_eq!(
        [gold[2], gold[3], gold[6], gold[9], gold[12]],
        [0, 5, 10, 15, 20]
    );
    let positions: Vec<Position> = workers
        .iter()
        .map(|&id| *mine.shop.sim.get::<Position>(id))
        .collect();
    assert_eq!(positions, [-2, 0, 2].map(|x| half(x, 8)));
}

#[test]
fn a_worker_at_a_taken_node_goes_to_the_nearest_free_one_near_it_or_waits() {
    // Minerals at (0, 6), the one ordered, at (3.5, 6), 1.5 m from it, at (−5, 6), 3 m, and at
    // (12, 6), 10 m, past the bounce of 4 m. The first worker, at (0.5, 4), takes the first; the
    // second, at (−0.5, 4), finds it taken, and goes on to the nearest free node within 4 m of
    // it: to (3.5, 6), whose box is √11.25 m off, not to (−5, 6), √14.5 m off.
    let mut mine = Mine::new();
    let node = mine.mineral(half(0, 12), 100);
    let east = mine.mineral(half(7, 12), 100);
    mine.mineral(half(-10, 12), 100);
    mine.mineral(half(24, 12), 100);
    let first = mine.worker(half(1, 8), 0);
    let second = mine.worker(half(-1, 8), 0);
    mine.gather_at(&[first, second], node);
    assert_eq!(mine.holder(node), Some(first));
    assert_eq!(
        (mine.node_of(second), mine.step(second)),
        (Some(east), Some(GatherStep::ToNode))
    );

    // With only the far node free, the second waits at its own.
    let mut mine = Mine::new();
    let node = mine.mineral(half(0, 12), 100);
    mine.mineral(half(24, 12), 100);
    let first = mine.worker(half(1, 8), 0);
    let second = mine.worker(half(-1, 8), 0);
    mine.gather_at(&[first, second], node);
    let waits = GatherStep::Waiting {
        since: Tick::new(0),
    };
    assert_eq!(
        (mine.node_of(second), mine.step(second)),
        (Some(node), Some(waits))
    );
}

#[test]
fn a_node_frees_to_its_waiting_worker_as_its_holder_dies_or_takes_another_order() {
    // The first worker holds the mineral, and the second waits from tick 0. As tick 1's loop
    // runs, the holder is dead, or its loop ended by a move order of tick 1: the waiting worker
    // holds the node, and its gather runs from tick 1.
    for dies in [true, false] {
        let mut mine = Mine::new();
        let node = mine.mineral(half(0, 12), 100);
        let first = mine.worker(half(1, 8), 0);
        let second = mine.worker(half(-1, 8), 0);
        mine.gather_at(&[first, second], node);
        if dies {
            mine.shop.sim.insert(first, Dead);
            mine.shop.tick();
        } else {
            let away = Action::Move {
                x: Num::int(10),
                z: Num::ZERO,
            };
            let payload = Order::payload(&[Order::one(first, away)]);
            mine.shop
                .sim
                .world
                .resource_mut::<TickInputs>()
                .push(TickInput {
                    slot: PlayerSlot::new(0),
                    payload: &payload,
                });
            mine.shop.sim.step();
        }
        let gathers = GatherStep::Gathering {
            since: Tick::new(1),
        };
        assert_eq!(mine.holder(node), Some(second), "dies {dies}");
        assert_eq!(mine.step(second), Some(gathers), "dies {dies}");
    }
}

#[test]
fn a_block_of_use_ends_a_gather_with_no_load_and_the_loop_starts_again_as_it_ends() {
    // A worker in range gathers from tick 0. In tick 1 a tag blocks its `use` group: its gather
    // ends with no load, and the node frees. Its loop waits while blocked; once the block ends,
    // in tick 3, it takes the node again, its gather running from tick 3.
    let mut mine = Mine::new();
    let node = mine.mineral(half(0, 12), 100);
    let worker = mine.worker(half(1, 8), 0);
    mine.gather_at(&[worker], node);
    mine.shop
        .sim
        .insert(worker, UnitTags::blocking(&[Block::Use]));
    mine.shop.tick();
    assert_eq!(
        (mine.holder(node), mine.step(worker)),
        (None, Some(GatherStep::ToNode))
    );
    mine.shop.tick();
    assert_eq!(mine.step(worker), Some(GatherStep::ToNode));
    let entity = mine.shop.sim.entity(worker);
    mine.shop.sim.world.entity_mut(entity).remove::<UnitTags>();
    mine.shop.tick();
    let gathers = GatherStep::Gathering {
        since: Tick::new(3),
    };
    assert_eq!(
        (mine.holder(node), mine.step(worker)),
        (Some(worker), Some(gathers))
    );
    assert_eq!(mine.shop.sim.get::<Gatherer>(worker).load(), None);
}

#[test]
fn a_load_past_what_an_amount_holds_stays_with_its_worker() {
    // The player holds 2 short of the most: the first load, 5, would pass it, so the worker,
    // in range of the hall, stands with it, and its loop keeps it at the drop-off. A worker no
    // player owns has no drop-off, and stands with its load too.
    let mut mine = Mine::new();
    mine.hall(half(0, 3));
    let node = mine.mineral(half(0, 12), 100);
    let worker = mine.worker(half(1, 8), 0);
    let near_most = i64::MAX - 2;
    let mut resources = mine.shop.sim.world.resource_mut::<PlayerResources>();
    resources
        .add(PlayerSlot::new(0), mine.gold, near_most)
        .unwrap();
    mine.gather_at(&[worker], node);
    for _ in 1..=4 {
        mine.shop.tick();
    }
    let stands = |mine: &Mine| {
        assert_eq!(mine.gold(), near_most);
        let gatherer = *mine.shop.sim.get::<Gatherer>(worker);
        assert_eq!(gatherer.load().map(|load| load.amount), Some(5));
        assert!(matches!(
            mine.step(worker),
            Some(GatherStep::ToDropOff { .. })
        ));
        let to = mine.shop.sim.try_get::<Destination>(worker).copied();
        assert_eq!(to.and_then(Destination::get), None);
    };
    stands(&mine);
    let entity = mine.shop.sim.entity(worker);
    mine.shop.sim.world.entity_mut(entity).remove::<Owner>();
    mine.shop.tick();
    stands(&mine);
}

#[test]
fn a_worker_sent_to_a_drop_off_returns_its_load_then_goes_back_and_drops_another_resource() {
    // A worker that gathered the mineral carries 5 when its loop is cut by a move. Sent to the
    // hall, in range, it delivers at once and goes back to the mineral it gathered last, 1.5 m
    // off, though a free one spans x 1.5 to 3.5 at z 4, 1 m off and within 4 m of it.
    let mut mine = Mine::new();
    let hall = mine.hall(half(0, 3));
    let node = mine.mineral(half(0, 12), 100);
    let worker = mine.worker(half(1, 8), 0);
    mine.gather_at(&[worker], node);
    for _ in 1..=3 {
        mine.shop.tick();
    }
    let nearer = mine.mineral(half(5, 8), 100);
    let mut gatherer = mine.shop.sim.get_mut::<Gatherer>(worker);
    gatherer.set(None);
    gatherer.carry(Some(Load {
        resource: mine.gold,
        amount: 5,
    }));
    mine.gather_at(&[worker], hall);
    assert_eq!(mine.gold(), 10);
    assert_eq!(mine.node_of(worker), Some(node));
    assert_eq!(mine.holder(nearer), None);
    assert_eq!(mine.shop.sim.get::<Gatherer>(worker).load(), None);

    // A load of another resource, sent to gather gold, is dropped.
    let wood = ResourceId::named(
        &["gold", "wood"].map(|name| DeclaredName::new(name).unwrap()),
        "wood",
    )
    .unwrap();
    let mut gatherer = mine.shop.sim.get_mut::<Gatherer>(worker);
    gatherer.set(None);
    gatherer.carry(Some(Load {
        resource: wood,
        amount: 3,
    }));
    mine.gather_at(&[worker], node);
    assert_eq!(mine.shop.sim.get::<Gatherer>(worker).load(), None);
}

#[test]
fn a_node_that_runs_out_mid_trip_sends_its_worker_to_the_nearest_free_one() {
    // The mineral at (0, 6) holds 5, one trip; another at (3.5, 6), 1.5 m off, holds 100. The
    // worker, in range of the first and of the hall, takes the 5 as tick 3's gather ends and
    // delivers them; the first, with nothing left, is gone to the loop, so the worker goes on
    // to the second in the same tick, and the first despawns as the tick ends.
    let mut mine = Mine::new();
    mine.hall(half(0, 3));
    let node = mine.mineral(half(0, 12), 5);
    let east = mine.mineral(half(7, 12), 100);
    let worker = mine.worker(half(1, 8), 0);
    mine.gather_at(&[worker], node);
    for _ in 1..=3 {
        mine.shop.tick();
    }
    assert_eq!((mine.gold(), mine.amount(node)), (5, None));
    assert_eq!(
        (mine.node_of(worker), mine.step(worker)),
        (Some(east), Some(GatherStep::ToNode))
    );
}

#[test]
fn a_unit_trained_toward_a_node_gathers_there_by_its_first_gather() {
    // A barracks whose rally point is the mineral trains a worker at once; it spawns with the
    // gather in its slot 0, which gathers the mineral's gold, and its loop goes to the mineral
    // as the next tick's orders are checked.
    let mut mine = Mine::new();
    let node = mine.mineral(half(0, 12), 100);
    let gather = mine.gather;
    let worker_type = mine.shop.grunt;
    let world = &mut mine.shop.sim.world;
    world.insert_non_send(Spawner::new(move |world, at, owner| {
        let slots = ActionSlots::new([(gather, SlotKind::new(0), Rank::new(1))]);
        let parts = (
            at.id,
            at.unit_type,
            at.team,
            at.pos,
            Body::new(Num::HALF).unwrap(),
            Navigation::walker(MoveStep::new(Num::ONE).unwrap()),
            slots,
            Gatherer::default(),
        );
        let mut unit = world.spawn(parts);
        if let Some(owner) = owner {
            unit.insert(Owner::new(owner));
        }
        assert_eq!(at.unit_type, worker_type);
        unit.id()
    }));
    let train = internals::train(world, worker_type, Ticks::ZERO, None);
    let barracks = mine.shop.id();
    let player = Some(PlayerSlot::new(0));
    mine.shop
        .producer(barracks, 0, -10, player, train, TrainQueue::default());
    mine.shop
        .sim
        .insert(barracks, Rally::new(RallyTarget::Unit(node)));
    mine.shop.order(barracks);
    mine.shop.tick();
    let index = mine.shop.sim.world.resource::<EntityIndex>();
    let trained = index.iter().map(|(id, _)| id).max().unwrap();
    assert_eq!(mine.step(trained), Some(GatherStep::Ordered));
    mine.shop.tick();
    assert_eq!(
        (mine.node_of(trained), mine.step(trained)),
        (Some(node), Some(GatherStep::ToNode))
    );
}
