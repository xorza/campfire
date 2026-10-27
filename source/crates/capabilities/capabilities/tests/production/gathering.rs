use std::num::{NonZeroI64, NonZeroU32};

use bevy_ecs::system::SystemState;

use campfire_actions::ActionBook;
use campfire_capabilities::Order;
use campfire_geometry::{Bounds, Grid};
use campfire_navigation::{FlightData, Navigation, Walker};
use campfire_orders::OrdersDirective;
use campfire_script::Filter;
use campfire_sim::{EntityIndex, MatchRate, Position};
use campfire_units::{GroundPoint, MapBook};
use campfire_values::{Block, RelationSet};

use super::*;
use crate::halves;

/// A mine's match: a worker's gather, and the type of the minerals it gathers at.
#[derive(Debug, Clone, Copy)]
struct Mine {
    gather: ActionId,
    mineral: UnitType,
    worker: UnitType,
}

/// How a mine's gather and minerals go: what a windup takes, the ticks a worker stands at a
/// drop-off, and what becomes of an empty mineral.
#[derive(Debug, Clone, Copy)]
struct Pace {
    unit: u32,
    deliver: u64,
    empty: EmptyNode,
}

/// A gathering scene: an ordering match with the shop's and the mine's types loaded.
#[derive(Debug)]
struct Mining {
    game: Match,
    shop: Shop,
    mine: Mine,
}

impl Mining {
    fn new() -> Self {
        let mut game = ordering_match();
        let shop = Shop::place(&mut game);
        let mine = Mine::place(&mut game, shop);
        Self { game, shop, mine }
    }
}

impl Mine {
    /// A match of production and its orders over 1 m cells from (−20, −20) to (20, 20), for walkers
    /// of a half meter. Its minerals are nodes of gold, boxes of 2 by 1 m of the neutral team 2,
    /// that go once they run out; its hall, player 0's, a box of 4 by 2 m, takes gold. A worker's
    /// gather takes 5 a trip in 3 ticks, within 1 m of its body's edge, and looks 4 m from its
    /// node for another.
    fn place(game: &mut Match, shop: Shop) -> Self {
        let pace = Pace {
            unit: 5,
            deliver: 0,
            empty: EmptyNode::Goes,
        };
        Self::place_paced(game, shop, pace)
    }

    /// The match of `place`, its gather and minerals as `pace` gives them.
    fn place_paced(game: &mut Match, shop: Shop, pace: Pace) -> Self {
        let bounds = Bounds::new([Num::int(-20); 2], [Num::int(20); 2]).unwrap();
        MapBook::amend(game, |_, map| map.bounds = bounds);
        let walker = Walker {
            layer: Layer::FIRST,
            radius: Num::HALF,
        };
        let grid = Grid::new(Num::ONE, bounds).unwrap();
        Navigation::load_pathing(game, grid, [walker]);
        let gold = shop.gold;
        Players::spawn(game, 1, 1);
        let mineral = Units::load_type(game, TypeScope::Mode, "mineral", &UnitTypeCore::default());
        let mut nodes = NodeBook::default();
        let node = NodeSpec {
            resource: gold,
            empty: pace.empty,
        };
        nodes.nodes.set(mineral, node);
        nodes.drop_offs.set(shop.depot, ResourceSet::from([gold]));
        game.insert_resource(nodes);
        let spec = GatherSpec {
            resource: gold,
            take: NonZeroU32::new(5).unwrap(),
            unit: NonZeroU32::new(pace.unit).unwrap(),
            bounce: Num::int(4),
            deliver: Ticks::new(pace.deliver),
        };
        let all = Filter::of_release_relations(RelationSet::All);
        let gather = Production::add_gather(game, spec, all, Num::ONE, Ticks::new(3));
        // Minerals are boxes of 2 by 1 m; workers, of a half meter, walk a meter a tick.
        let mineral_body = BodyForm::new(
            BodyShape::box_sized([Num::int(2), Num::ONE]).unwrap(),
            Layer::FIRST,
        );
        Units::give_type(game, mineral, mineral_body);
        let worker = Units::load_type(game, TypeScope::Mode, "worker", &UnitTypeCore::default());
        Units::give_type(
            game,
            worker,
            BodyForm::new(BodyShape::circle(Num::HALF).unwrap(), Layer::FIRST),
        );
        Units::give_type(game, worker, MoveStep::new(Num::ONE).unwrap());
        Units::give_type(game, worker, Gatherer::default());
        Self {
            gather,
            mineral,
            worker,
        }
    }

    /// A worker of player `owner` at `position`, its gather learned.
    fn worker(self, game: &mut World, position: impl Into<Vec3>, owner: u32) -> StableId {
        let position = position.into();
        self.worker_ranked(game, position, owner, Rank::new(1))
    }

    /// A worker as `worker` gives, its gather at `rank`, or not learned with none.
    fn worker_ranked(
        self,
        game: &mut World,
        position: impl Into<Vec3>,
        owner: u32,
        rank: Option<Rank>,
    ) -> StableId {
        let position = position.into();
        let gather = self.gather;
        let slots = TimedSlots::new(ActionSlots::new([(gather, SlotKind::new(0), rank)]));
        let parts = (Owner::new(PlayerSlot::new(owner)), slots);
        Spawn::of(self.worker, position).parts(parts).place(game)
    }

    /// A mineral at `position` holding `amount`.
    fn mineral(self, game: &mut World, position: impl Into<Vec3>, amount: u32) -> StableId {
        Spawn::of(self.mineral, position)
            .team(Team::new(2))
            .parts(Node::new(amount))
            .place(game)
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
    let Mining {
        mut game,
        shop,
        mine,
    } = Mining::new();
    shop.hall(&mut game, halves(0, 0));
    let node = mine.mineral(&mut game, halves(0, 12), 12);
    let worker = mine.worker(&mut game, halves(1, 6), 0);
    // A worker far off whose gather it has not learned holds no gather: its order stops in the
    // tick it applies.
    let unlearned = mine.worker_ranked(&mut game, halves(-20, 6), 0, None);
    Production::gather_at(&mut game, &[worker, unlearned], node);
    let mut seen = Vec::new();
    for tick in 1..=21 {
        game.step();
        seen.push((
            shop.gold(&game),
            game.try_part::<Node>(node).map(|node| node.amount()),
        ));
        if tick == 1 {
            assert_eq!(Production::gather_step(&game, unlearned), None);
        }
    }
    let at = |tick: usize| seen[tick - 1];
    assert_eq!(at(1), (0, Some(12)));
    assert_eq!([at(4), at(5)], [(0, Some(7)), (0, Some(7))]);
    assert_eq!([at(6), at(10)], [(5, Some(7)), (5, Some(7))]);
    assert_eq!([at(11), at(13)], [(5, Some(2)), (10, Some(2))]);
    assert_eq!([at(17), at(18)], [(10, Some(2)), (10, None)]);
    assert_eq!([at(19), at(20)], [(10, None), (12, None)]);
    assert_eq!(Production::gather_step(&game, worker), None);
    assert_eq!(game.part::<Gatherer>(worker).load(), None);
    // The loop's rule of a node, as a bot's search runs it: a worker's gather, learned or not,
    // takes a mineral with gold left, and neither an empty one nor a unit that is no node.
    let full = mine.mineral(&mut game, halves(10, 12), 12);
    let empty = mine.mineral(&mut game, halves(-10, 12), 0);
    let mut view = SystemState::<GatherView<'_, '_>>::new(&mut game);
    let view = view.get(&game).unwrap();
    assert!(view.gathers(worker, 0, full) && view.gathers(unlearned, 0, full));
    assert!(!view.gathers(worker, 0, empty) && !view.gathers(worker, 0, worker));

    // On a predicting client, which holds the mineral as the server sent it: its worker takes the
    // mineral and carries 5 of it from tick 4 as the server's does, and the mineral keeps its 12
    // and its holder, none, the server's word.
    let mut client = Match::builder(&ORDERING).role(Role::Client).build();
    let shop = Shop::place(&mut client);
    let mine = Mine::place(&mut client, shop);
    shop.hall(&mut client, halves(0, 0));
    let node = mine.mineral(&mut client, halves(0, 12), 12);
    client.insert_parts(node, Unpredicted);
    let worker = mine.worker(&mut client, halves(1, 6), 0);
    Production::gather_at(&mut client, &[worker], node);
    let mut loads = Vec::new();
    for _ in 1..=4 {
        client.step();
        loads.push(client.part::<Gatherer>(worker).load());
    }
    let five = Some(Load {
        resource: shop.gold,
        amount: 5,
    });
    assert_eq!(loads, [None, None, None, five]);
    assert_eq!(*client.part::<Node>(node), Node::new(12));
    // It takes its load to the hall and drops it in tick 6, as the server's does, and joins
    // nothing to its player's gold, which is the server's.
    let gold = shop.gold(&client);
    let delivered = (5..=40).find(|_| {
        client.step();
        client.part::<Gatherer>(worker).load().is_none()
    });
    assert_eq!(delivered, Some(6));
    assert_eq!(shop.gold(&client), gold);
}

#[test]
fn a_flier_gathers_as_a_walker_does_at_its_height() {
    // The scene of the walker's first trips, its worker a flier of the air layer, of a flight at
    // 8 m that climbs a meter a tick: it reaches the mineral and the hall by its reach on the
    // ground between them, whatever its height, so its trips take the walker's ticks: 5 gold in
    // tick 6, 10 in tick 13 (helicopters.md, A flying gatherer docks over its dock).
    let Mining {
        mut game,
        shop,
        mine,
    } = Mining::new();
    let bounds = Bounds::new([Num::int(-20); 2], [Num::int(20); 2]).unwrap();
    let [ground, air] = [Layer::FIRST, Layer::new(1)].map(|layer| Walker {
        layer,
        radius: Num::HALF,
    });
    Navigation::load_pathing(
        &mut game,
        Grid::new(Num::ONE, bounds).unwrap(),
        [ground, air],
    );
    let flier = Units::load_type(
        &mut game,
        TypeScope::Mode,
        "flier",
        &UnitTypeCore::default(),
    );
    let body = BodyForm::new(BodyShape::circle(Num::HALF).unwrap(), air.layer);
    Units::give_type(&mut game, flier, body);
    Units::give_type(&mut game, flier, MoveStep::new(Num::ONE).unwrap());
    Units::give_type(&mut game, flier, Gatherer::default());
    let hz = NonZeroI64::from(game.resource::<MatchRate>().hz()).get();
    let flight = FlightData::new(Num::int(8), Num::int(hz), Num::int(3)).unwrap();
    Navigation::load_flight(&mut game, flier, flight);
    shop.hall(&mut game, halves(0, 0));
    let node = mine.mineral(&mut game, halves(0, 12), 12);
    let slots = TimedSlots::new(ActionSlots::new([(
        mine.gather,
        SlotKind::new(0),
        Rank::new(1),
    )]));
    let parts = (Owner::new(PlayerSlot::new(0)), slots);
    let worker = Spawn::of(flier, halves(1, 6)).parts(parts).place(&mut game);
    Production::gather_at(&mut game, &[worker], node);
    let mut gold = Vec::new();
    for _ in 1..=13 {
        game.step();
        gold.push(shop.gold(&game));
    }
    assert_eq!([gold[4], gold[5], gold[11], gold[12]], [0, 5, 5, 10]);
    assert_eq!(game.part::<Position>(worker).get().y, Num::int(8));
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
    let Mining {
        mut game,
        shop,
        mine,
    } = Mining::new();
    shop.hall(&mut game, halves(0, 3));
    let node = mine.mineral(&mut game, halves(0, 12), 100);
    let workers = [-2, 0, 2].map(|x| mine.worker(&mut game, halves(x, 8), 0));
    Production::gather_at(&mut game, &workers, node);
    let mut holders = vec![game.part::<Node>(node).holder()];
    let mut gold = vec![shop.gold(&game)];
    for _ in 1..=12 {
        game.step();
        holders.push(game.part::<Node>(node).holder());
        gold.push(shop.gold(&game));
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
    let positions: Vec<Vec3> = workers
        .iter()
        .map(|&id| game.part::<Position>(id).get())
        .collect();
    assert_eq!(positions, [-2, 0, 2].map(|x| halves(x, 8)));
}

#[test]
fn a_worker_at_a_taken_node_goes_to_the_nearest_free_one_near_it_or_waits() {
    // Minerals at (0, 6), the one ordered, at (3.5, 6), 1.5 m from it, at (−5, 6), 3 m, and at
    // (12, 6), 10 m, past the bounce of 4 m. The first worker, at (0.5, 4), takes the first; the
    // second, at (−0.5, 4), finds it taken, and goes on to the nearest free node within 4 m of
    // it: to (3.5, 6), whose box is √11.25 m off, not to (−5, 6), √14.5 m off.
    let Mining { mut game, mine, .. } = Mining::new();
    let node = mine.mineral(&mut game, halves(0, 12), 100);
    let east = mine.mineral(&mut game, halves(7, 12), 100);
    mine.mineral(&mut game, halves(-10, 12), 100);
    mine.mineral(&mut game, halves(24, 12), 100);
    let first = mine.worker(&mut game, halves(1, 8), 0);
    let second = mine.worker(&mut game, halves(-1, 8), 0);
    Production::gather_at(&mut game, &[first, second], node);
    assert_eq!(game.part::<Node>(node).holder(), Some(first));
    assert_eq!(
        (
            Production::node_of(&game, second),
            Production::gather_step(&game, second)
        ),
        (Some(east), Some(GatherStep::ToNode))
    );

    // With only the far node free, the second waits at its own.
    let Mining { mut game, mine, .. } = Mining::new();
    let node = mine.mineral(&mut game, halves(0, 12), 100);
    mine.mineral(&mut game, halves(24, 12), 100);
    let first = mine.worker(&mut game, halves(1, 8), 0);
    let second = mine.worker(&mut game, halves(-1, 8), 0);
    Production::gather_at(&mut game, &[first, second], node);
    let waits = GatherStep::Waiting {
        since: Tick::new(0),
    };
    assert_eq!(
        (
            Production::node_of(&game, second),
            Production::gather_step(&game, second)
        ),
        (Some(node), Some(waits))
    );
}

#[test]
fn a_node_frees_to_its_waiting_worker_as_its_holder_dies_or_takes_another_order() {
    // The first worker holds the mineral, and the second waits from tick 0. As tick 1's loop
    // runs, the holder is dead, or its loop ended by a move order of tick 1: the waiting worker
    // holds the node, and its gather runs from tick 1.
    for dies in [true, false] {
        let Mining { mut game, mine, .. } = Mining::new();
        let node = mine.mineral(&mut game, halves(0, 12), 100);
        let first = mine.worker(&mut game, halves(1, 8), 0);
        let second = mine.worker(&mut game, halves(-1, 8), 0);
        Production::gather_at(&mut game, &[first, second], node);
        if dies {
            game.insert_parts(first, Dead);
            game.step();
        } else {
            let away = OrdersDirective::Move {
                to: GroundPoint {
                    x: Num::int(10),
                    z: Num::ZERO,
                },
            };
            let payload = Order::payload(&[Order::one(first, away)]);
            game.step_with(&[(PlayerSlot::new(0), &payload)]);
        }
        let gathers = GatherStep::Gathering {
            since: Tick::new(1),
        };
        assert_eq!(
            game.part::<Node>(node).holder(),
            Some(second),
            "dies {dies}"
        );
        assert_eq!(
            Production::gather_step(&game, second),
            Some(gathers),
            "dies {dies}"
        );
    }
}

#[test]
fn a_block_of_use_ends_a_gather_with_no_load_and_the_loop_starts_again_as_it_ends() {
    // A worker in range gathers from tick 0. In tick 1 a tag blocks its `use` group: its gather
    // ends with no load, and the node frees. Its loop waits while blocked; once the block ends,
    // in tick 3, it takes the node again, its gather running from tick 3.
    let Mining { mut game, mine, .. } = Mining::new();
    let node = mine.mineral(&mut game, halves(0, 12), 100);
    let worker = mine.worker(&mut game, halves(1, 8), 0);
    Production::gather_at(&mut game, &[worker], node);
    Units::set_blocks(&mut game, worker, &[Block::Use]);
    game.step();
    assert_eq!(
        (
            game.part::<Node>(node).holder(),
            Production::gather_step(&game, worker)
        ),
        (None, Some(GatherStep::ToNode))
    );
    game.step();
    assert_eq!(
        Production::gather_step(&game, worker),
        Some(GatherStep::ToNode)
    );
    Units::set_blocks(&mut game, worker, &[]);
    game.step();
    let gathers = GatherStep::Gathering {
        since: Tick::new(3),
    };
    assert_eq!(
        (
            game.part::<Node>(node).holder(),
            Production::gather_step(&game, worker)
        ),
        (Some(worker), Some(gathers))
    );
    assert_eq!(game.part::<Gatherer>(worker).load(), None);
}

#[test]
fn a_gather_whose_when_its_worker_fails_ends_with_no_load_and_starts_again_as_it_passes() {
    // The gather needs a worker of no `test_blocks_move` tag, whose block of moving keeps no
    // worker from a gather. A worker in range gathers from tick 0, its order's; in tick 2 it takes
    // the tag: its gather ends, and the node frees, as a block of `use` ends it. Once it loses the
    // tag, in tick 3, it takes the node again, its gather running from tick 3.
    let Mining { mut game, mine, .. } = Mining::new();
    Release::load_test_modifiers(&mut game);
    let unmet = Filter::of_match(&game, "all:!test_blocks_move");
    ActionBook::amend(&mut game, |_, book| book.set_when(mine.gather, Some(unmet)));
    let node = mine.mineral(&mut game, halves(0, 12), 100);
    let worker = mine.worker(&mut game, halves(1, 8), 0);
    Production::gather_at(&mut game, &[worker], node);
    let state = |game: &Match| {
        (
            game.part::<Node>(node).holder(),
            Production::gather_step(game, worker),
        )
    };
    let gathers = |since| {
        Some(GatherStep::Gathering {
            since: Tick::new(since),
        })
    };
    game.step();
    assert_eq!(state(&game), (Some(worker), gathers(0)));
    Units::set_blocks(&mut game, worker, &[Block::Move]);
    game.step();
    assert_eq!(state(&game), (None, Some(GatherStep::ToNode)));
    Units::set_blocks(&mut game, worker, &[]);
    game.step();
    assert_eq!(state(&game), (Some(worker), gathers(3)));
    assert_eq!(game.part::<Gatherer>(worker).load(), None);
}

#[test]
fn a_load_past_what_an_amount_holds_stays_with_its_worker() {
    // The player holds 2 short of the most: the first load, 5, would pass it, so the worker,
    // in range of the hall, stands with it, and its loop keeps it at the drop-off. A worker no
    // player owns has no drop-off, and stands with its load too.
    let Mining {
        mut game,
        shop,
        mine,
    } = Mining::new();
    shop.hall(&mut game, halves(0, 3));
    let node = mine.mineral(&mut game, halves(0, 12), 100);
    let worker = mine.worker(&mut game, halves(1, 8), 0);
    let near_most = i64::MAX - 2;
    let resources = game.player_resources_mut(PlayerSlot::new(0));
    resources.unwrap().add(shop.gold, near_most).unwrap();
    Production::gather_at(&mut game, &[worker], node);
    for _ in 1..=4 {
        game.step();
    }
    let stands = |game: &Match| {
        assert_eq!(shop.gold(game), near_most);
        let gatherer = *game.part::<Gatherer>(worker);
        assert_eq!(gatherer.load().map(|load| load.amount), Some(5));
        assert!(matches!(
            Production::gather_step(game, worker),
            Some(GatherStep::ToDropOff { .. })
        ));
        let to = game.try_part::<Destination>(worker).copied();
        assert_eq!(to.and_then(Destination::get), None);
    };
    stands(&game);
    let entity = game.unit_entity(worker);
    game.entity_mut(entity).remove::<Owner>();
    game.step();
    stands(&game);
}

#[test]
fn a_worker_sent_to_a_drop_off_returns_its_load_then_goes_back_and_drops_another_resource() {
    // A worker that gathered the mineral carries 5 when its loop is cut by a move. Sent to the
    // hall, in range, it delivers at once and goes back to the mineral it gathered last, 1.5 m
    // off, though a free one spans x 1.5 to 3.5 at z 4, 1 m off and within 4 m of it.
    let Mining {
        mut game,
        shop,
        mine,
    } = Mining::new();
    let hall = shop.hall(&mut game, halves(0, 3));
    let node = mine.mineral(&mut game, halves(0, 12), 100);
    let worker = mine.worker(&mut game, halves(1, 8), 0);
    Production::gather_at(&mut game, &[worker], node);
    for _ in 1..=3 {
        game.step();
    }
    let nearer = mine.mineral(&mut game, halves(5, 8), 100);
    let mut gatherer = game.part_mut::<Gatherer>(worker);
    gatherer.set_unindexed(None);
    gatherer.carry(Some(Load {
        resource: shop.gold,
        amount: 5,
    }));
    Production::gather_at(&mut game, &[worker], hall);
    assert_eq!(shop.gold(&game), 10);
    assert_eq!(Production::node_of(&game, worker), Some(node));
    assert_eq!(game.part::<Node>(nearer).holder(), None);
    assert_eq!(game.part::<Gatherer>(worker).load(), None);

    // A load of another resource, sent to gather gold, is dropped.
    let wood = ResourceId::new(1);
    let mut gatherer = game.part_mut::<Gatherer>(worker);
    gatherer.set_unindexed(None);
    gatherer.carry(Some(Load {
        resource: wood,
        amount: 3,
    }));
    Production::gather_at(&mut game, &[worker], node);
    assert_eq!(game.part::<Gatherer>(worker).load(), None);
}

#[test]
fn a_node_that_runs_out_mid_trip_sends_its_worker_to_the_nearest_free_one() {
    // The mineral at (0, 6) holds 5, one trip; another at (3.5, 6), 1.5 m off, holds 100. The
    // worker, in range of the first and of the hall, takes the 5 as tick 3's gather ends and
    // delivers them; the first, with nothing left, is gone to the loop, so the worker goes on
    // to the second in the same tick, and the first despawns as the tick ends.
    let Mining {
        mut game,
        shop,
        mine,
    } = Mining::new();
    shop.hall(&mut game, halves(0, 3));
    let node = mine.mineral(&mut game, halves(0, 12), 5);
    let east = mine.mineral(&mut game, halves(7, 12), 100);
    let worker = mine.worker(&mut game, halves(1, 8), 0);
    Production::gather_at(&mut game, &[worker], node);
    for _ in 1..=3 {
        game.step();
    }
    assert_eq!(
        (
            shop.gold(&game),
            game.try_part::<Node>(node).map(|node| node.amount())
        ),
        (5, None)
    );
    assert_eq!(
        (
            Production::node_of(&game, worker),
            Production::gather_step(&game, worker)
        ),
        (Some(east), Some(GatherStep::ToNode))
    );
}

#[test]
fn a_unit_trained_toward_a_node_gathers_there_by_its_first_gather() {
    // A barracks whose rally point is the mineral trains a worker at once; it spawns with the
    // gather in its slot 0, which gathers the mineral's gold, and its loop goes to the mineral
    // as the next tick's orders are checked.
    let Mining {
        mut game,
        shop,
        mine,
    } = Mining::new();
    let node = mine.mineral(&mut game, halves(0, 12), 100);
    let gather = mine.gather;
    let worker_type = shop.grunt;
    let slots = ActionSlots::new([(gather, SlotKind::new(0), Rank::new(1))]);
    Shop::give(&mut game, worker_type, slots);
    Shop::give(&mut game, worker_type, Gatherer::default());
    let train = Production::add_train(&mut game, worker_type, Ticks::ZERO, None);
    let barracks = Production::allocate_id(&mut game);
    let player = Some(PlayerSlot::new(0));
    shop.producer(
        &mut game,
        barracks,
        -10,
        Team::new(0),
        player,
        train,
        TrainQueue::default(),
    );
    game.insert_parts(barracks, Rally::new(Some(RallyTarget::Unit(node))));
    Production::order_train(&mut game, barracks);
    game.step();
    let index = game.resource::<EntityIndex>();
    let units = index
        .iter()
        .map(|(id, _)| id)
        .filter(|id| id.is_allocated());
    let trained = units.max().unwrap();
    assert_eq!(
        Production::gather_step(&game, trained),
        Some(GatherStep::Ordered)
    );
    game.step();
    assert_eq!(
        (
            Production::node_of(&game, trained),
            Production::gather_step(&game, trained)
        ),
        (Some(node), Some(GatherStep::ToNode))
    );
}

#[test]
fn a_predicting_clients_gather_loop_reads_only_what_it_holds() {
    // The client predicts player 0's hall and worker, and holds as the server sent them a mineral
    // and a rival worker that gathers it. Its worker is ordered to the same mineral.
    Release::sync_held(&ORDERING, 24, |game| {
        let shop = Shop::place(game);
        let mine = Mine::place(game, shop);
        shop.hall(game, halves(0, 0));
        let node = mine.mineral(game, halves(0, 12), 100);
        let rival = mine.worker(game, halves(2, 10), 0);
        Production::gather_at(game, &[rival], node);
        game.step();
        game.insert_parts(node, Unpredicted);
        game.insert_parts(rival, Unpredicted);
        let worker = mine.worker(game, halves(1, 6), 0);
        Production::gather_at(game, &[worker], node);
    });
}

#[test]
fn a_worker_that_dies_or_loses_its_node_ends_its_gather_and_frees_the_node() {
    // As in the first test, tick 0 walks the worker to the mineral and tick 1 takes it. In tick
    // 2 the worker dies: its order ends, as a dead unit takes none, and the mineral frees, so the
    // worker gathers from no node as it respawns. Or the mineral dies: the worker finds no node
    // it gathers near it and stops, and frees the mineral in that tick's step.
    for worker_dies in [true, false] {
        let Mining {
            mut game,
            shop,
            mine,
        } = Mining::new();
        shop.hall(&mut game, halves(0, 0));
        let node = mine.mineral(&mut game, halves(0, 12), 12);
        let worker = mine.worker(&mut game, halves(1, 6), 0);
        Production::gather_at(&mut game, &[worker], node);
        game.step();
        game.step();
        assert!(matches!(
            Production::gather_step(&game, worker),
            Some(GatherStep::Gathering { .. })
        ));
        assert_eq!(game.part::<Node>(node).holder(), Some(worker));
        game.insert_parts(if worker_dies { worker } else { node }, Dead);
        game.step();
        assert_eq!(
            Production::gather_step(&game, worker),
            None,
            "{worker_dies}"
        );
        assert_eq!(game.part::<Node>(node).holder(), None, "{worker_dies}");
    }
}

/// What a match shows after a tick: player 0's gold, what the mineral holds, none once it is
/// gone, and what the worker carries.
type Seen = (i64, Option<u32>, Option<u32>);

/// Gold, the mineral and the worker's load after each of ticks 0 to `ticks`, of a worker at
/// (0, 4), in range of a mineral of `amount` at (0, 6) and of the hall at (0, 1.5), so it never
/// walks, its gather paced by `pace` or left as it is with none.
fn gather_by_tick(amount: u32, pace: Option<(u32, u64)>, ticks: usize) -> Vec<Seen> {
    let mut game = ordering_match();
    let shop = Shop::place(&mut game);
    let (unit, deliver) = pace.unwrap_or((5, 0));
    let pace = Pace {
        unit,
        deliver,
        empty: EmptyNode::Goes,
    };
    let mine = Mine::place_paced(&mut game, shop, pace);
    shop.hall(&mut game, halves(0, 3));
    let node = mine.mineral(&mut game, halves(0, 12), amount);
    let worker = mine.worker(&mut game, halves(0, 8), 0);
    let seen = |game: &Match| {
        let load = game.part::<Gatherer>(worker).load();
        (
            shop.gold(game),
            game.try_part::<Node>(node).map(|node| node.amount()),
            load.map(|load| load.amount),
        )
    };
    Production::gather_at(&mut game, &[worker], node);
    let mut by_tick = vec![seen(&game)];
    for _ in 1..=ticks {
        game.step();
        by_tick.push(seen(&game));
    }
    by_tick
}

#[test]
fn a_gathers_unit_fills_a_trip_windup_by_windup_and_the_last_trip_takes_what_is_left() {
    // A trip carries 5, each windup of 3 ticks taking 2 of it; the worker takes the mineral of 12
    // in tick 0. Ticks 3 and 6 take 2 each, tick 9 the last 1 of the trip, which it delivers at
    // once, and takes the mineral again: 7 left. Ticks 12, 15 and 18 take 2, 2 and 1, the second
    // trip, 10 delivered; tick 21 takes the last 2, a short trip, and delivers 12 as the mineral
    // runs out and goes.
    let by_tick = gather_by_tick(12, Some((2, 0)), 21);
    let at = |ticks: [usize; 7]| ticks.map(|tick| by_tick[tick]);
    assert_eq!(
        at([0, 3, 6, 8, 9, 12, 15]),
        [
            (0, Some(12), None),
            (0, Some(10), Some(2)),
            (0, Some(8), Some(4)),
            (0, Some(8), Some(4)),
            (5, Some(7), None),
            (5, Some(5), Some(2)),
            (5, Some(3), Some(4)),
        ]
    );
    assert_eq!(
        [by_tick[18], by_tick[20], by_tick[21]],
        [(10, Some(2), None), (10, Some(2), None), (12, None, None)]
    );
    // With no unit, each windup takes the whole trip's 5: 5 by tick 3, 10 by 6, and the last 2
    // by 9.
    let whole = gather_by_tick(12, None, 9);
    assert_eq!(
        [whole[2], whole[3], whole[6], whole[9]],
        [
            (0, Some(12), None),
            (5, Some(7), None),
            (10, Some(2), None),
            (12, None, None)
        ]
    );
}

#[test]
fn a_worker_stands_its_gathers_time_at_the_drop_off_before_its_load_joins() {
    // The worker's windup ends in tick 3, and it reaches the hall at once: it stands there 2
    // ticks, and its 5 join the gold in tick 5, as it takes the mineral again; its next load
    // joins in tick 10. The mineral frees as its windup ends, though the worker stands with its
    // load. With no time at the drop-off, the loads join in ticks 3 and 6.
    let gold = |by_tick: &[Seen]| by_tick.iter().map(|seen| seen.0).collect::<Vec<_>>();
    let stands = gather_by_tick(100, Some((5, 2)), 10);
    assert_eq!(gold(&stands), [0, 0, 0, 0, 0, 5, 5, 5, 5, 5, 10]);
    let at_once = gather_by_tick(100, None, 10);
    assert_eq!(gold(&at_once), [0, 0, 0, 5, 5, 5, 10, 10, 10, 15, 15]);

    let mut game = ordering_match();
    let shop = Shop::place(&mut game);
    let pace = Pace {
        unit: 5,
        deliver: 2,
        empty: EmptyNode::Goes,
    };
    let mine = Mine::place_paced(&mut game, shop, pace);
    let hall = shop.hall(&mut game, halves(0, 3));
    let node = mine.mineral(&mut game, halves(0, 12), 100);
    let worker = mine.worker(&mut game, halves(0, 8), 0);
    Production::gather_at(&mut game, &[worker], node);
    for _ in 1..=3 {
        game.step();
    }
    let delivering = GatherStep::Delivering {
        drop_off: hall,
        since: Tick::new(3),
    };
    assert_eq!(Production::gather_step(&game, worker), Some(delivering));
    assert_eq!(game.part::<Node>(node).holder(), None);
}

#[test]
fn an_empty_node_that_stays_stands_and_no_worker_gathers_it() {
    // A mineral of 5, one trip, runs out in tick 3: one that goes despawns as the tick ends, and
    // one that stays stands, empty. A worker ordered to it then gathers nothing.
    for stays in [false, true] {
        let mut game = ordering_match();
        let shop = Shop::place(&mut game);
        let pace = Pace {
            unit: 5,
            deliver: 0,
            empty: if stays {
                EmptyNode::Stays
            } else {
                EmptyNode::Goes
            },
        };
        let mine = Mine::place_paced(&mut game, shop, pace);
        shop.hall(&mut game, halves(0, 3));
        let node = mine.mineral(&mut game, halves(0, 12), 5);
        let worker = mine.worker(&mut game, halves(0, 8), 0);
        Production::gather_at(&mut game, &[worker], node);
        for _ in 1..=3 {
            game.step();
        }
        assert_eq!(shop.gold(&game), 5, "stays {stays}");
        let left = game.try_part::<Node>(node).map(|node| node.amount());
        assert_eq!(left, stays.then_some(0), "stays {stays}");
        assert_eq!(
            Production::gather_step(&game, worker),
            None,
            "stays {stays}"
        );
        if stays {
            Production::gather_at(&mut game, &[worker], node);
            assert_eq!(Production::gather_step(&game, worker), None);
        }
    }
}

#[test]
fn a_producer_that_starts_its_gatherers_sends_each_to_the_nearest_free_node_within_its_bounce() {
    // A barracks at (−10, 0) with no exit trains a worker at once, which spawns in the cell by it
    // nearest its rally point at (−10, −8). Minerals stand at (−10, 3), at (−10, −4.5), farther
    // from the worker's cell, and at (−10, 8), past the bounce of 4 m from its body. A barracks
    // that starts its gatherers sends the worker to the nearest, though a rally point is set; one
    // that does not leaves it to the rally point. With only the mineral past the bounce, the
    // worker walks to the rally point.
    let near = halves(-20, 6);
    let farther = halves(-20, -9);
    let past = halves(-20, 16);
    let rally = GroundPoint {
        x: Num::int(-10),
        z: Num::int(-8),
    };
    let rally_point = Vec3::new(Num::int(-10), Num::ZERO, Num::int(-8));
    let cases = [
        (true, vec![near, farther, past], Some(0), None),
        (false, vec![near, farther, past], None, Some(rally_point)),
        (true, vec![past], None, Some(rally_point)),
    ];
    for (gathers, minerals, gathered, walks) in cases {
        let Mining {
            mut game,
            shop,
            mine,
        } = Mining::new();
        let nodes: Vec<StableId> = minerals
            .iter()
            .map(|&at| mine.mineral(&mut game, at, 100))
            .collect();
        let worker_type = shop.grunt;
        let slots = ActionSlots::new([(mine.gather, SlotKind::new(0), Rank::new(1))]);
        Shop::give(&mut game, worker_type, slots);
        Shop::give(&mut game, worker_type, Gatherer::default());
        let spec = ProducerSpec {
            queue: NonZeroU8::new(3).unwrap(),
            exit: None,
            rally: None,
            gathers,
            free: Box::new([]),
        };
        Shop::regive(&mut game, shop.barracks, spec);
        let train = Production::add_train(&mut game, worker_type, Ticks::ZERO, None);
        let barracks = Production::allocate_id(&mut game);
        let player = Some(PlayerSlot::new(0));
        let queue = TrainQueue::default();
        shop.producer(&mut game, barracks, -10, Team::new(0), player, train, queue);
        game.insert_parts(barracks, Rally::new(Some(RallyTarget::Point(rally))));
        Production::order_train(&mut game, barracks);
        game.step();
        let [trained] = <[Spawned; 1]>::try_from(Shop::spawned(&mut game, worker_type)).unwrap();
        let worker = trained.id;
        let to = game.part::<Destination>(worker).get();
        assert_eq!(to, walks, "gathers {gathers}, {minerals:?}");
        game.step();
        let node = gathered.map(|nth: usize| nodes[nth]);
        let step = node.map(|_| GatherStep::ToNode);
        assert_eq!(
            (
                Production::node_of(&game, worker),
                Production::gather_step(&game, worker)
            ),
            (node, step),
            "gathers {gathers}, {minerals:?}"
        );
    }
}
