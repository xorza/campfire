use campfire_sim::{EntityIndex, TickInput, TickInputs};

use super::*;
use crate::geometry::bounds::Bounds;
use crate::geometry::grid::Grid;
use crate::navigation::Navigation;
use crate::navigation::walker::Walker;
use crate::orders::order::{Action, Order};
use crate::production::rally_target::RallyTarget;
use crate::units::body::{Body, BodyForm};
use crate::units::layer::Layer;

impl Shop {
    /// Runs a tick in which player `slot` orders `action` to `unit`.
    fn tick_ordering(&mut self, slot: u32, unit: StableId, action: Action) {
        let payload = Order::payload(&[Order::one(unit, action)]);
        self.sim.world.resource_mut::<TickInputs>().push(TickInput {
            slot: PlayerSlot::new(slot),
            payload: &payload,
        });
        self.sim.step();
    }
}

/// The ground point at `x` and `z` half meters.
fn half(x: i64, z: i64) -> Position {
    Position::new(Vec3::new(Num::HALF * x, Num::ZERO, Num::HALF * z)).unwrap()
}

#[test]
fn a_cancel_refunds_what_its_entry_paid_and_a_cancelled_head_starts_the_next() {
    // Player 0 holds 0 gold; its barracks queues three trains of 4 ticks, pushed at tick 0, that
    // paid 10, 20 and 30 gold: the head is done at time 4.
    let mut shop = Shop::ordering();
    let gold = ResourceId::named(&[DeclaredName::new("gold").unwrap()], "gold").unwrap();
    shop.sim.world.insert_resource(PlayerResources::new(1, 1));
    let train = internals::train(&mut shop.sim.world, shop.grunt, Ticks::new(4), None);
    let mut queue = TrainQueue::default();
    for paid in [10, 20, 30] {
        let queued = Queued {
            action: train,
            rank: Rank::FIRST,
            time: Ticks::new(4),
            paid: 1,
        };
        let amount = ResourceAmount {
            resource: gold,
            amount: paid,
        };
        queue.push(queued, &[amount], Tick::new(0));
    }
    let player = PlayerSlot::new(0);
    let producer = shop.id();
    shop.producer(producer, 1, 10, Some(player), train, queue);
    let held = |shop: &Shop| {
        let amount = shop
            .sim
            .world
            .resource::<PlayerResources>()
            .amount(player, gold);
        (amount, shop.queued(producer), shop.spawns().len())
    };
    let cancel = |place| Action::CancelTrain { place };
    // Tick 0: the entry behind the head gives back its 20; the head keeps its time. Another
    // player's cancel, and one past the queue's end, do nothing.
    shop.tick_ordering(0, producer, cancel(1));
    assert_eq!(held(&shop), (20, 2, 0));
    shop.tick_ordering(1, producer, cancel(0));
    shop.tick_ordering(0, producer, cancel(2));
    assert_eq!(held(&shop), (20, 2, 0));
    // Tick 3: the head gives back its 10, and the last starts at time 3, to be done at time 7,
    // the end of tick 6, not at time 4.
    shop.tick_ordering(0, producer, cancel(0));
    assert_eq!(held(&shop), (30, 1, 0));
    for tick in 4..=6 {
        assert_eq!(held(&shop).2, 0, "tick {tick}");
        shop.tick();
    }
    assert_eq!(held(&shop), (30, 0, 1));

    // A refund that would carry the amount past an `i64` refuses the cancel: the entry stays.
    let mut queue = TrainQueue::default();
    let queued = Queued {
        action: train,
        rank: Rank::FIRST,
        time: Ticks::new(4),
        paid: 1,
    };
    let ten = ResourceAmount {
        resource: gold,
        amount: 10,
    };
    queue.push(queued, &[ten], Tick::new(7));
    shop.sim.insert(producer, queue);
    let mut resources = shop.sim.world.resource_mut::<PlayerResources>();
    resources.add(player, gold, i64::MAX - 35).unwrap();
    shop.tick_ordering(0, producer, cancel(0));
    assert_eq!(held(&shop), (i64::MAX - 5, 1, 1));
}

#[test]
fn a_trained_unit_spawns_on_its_rally_points_side_of_its_producer_and_goes_there() {
    // Over 1 m cells from (−10, −10) to (10, 10), a barracks of a box 4 by 2 m at the origin
    // trains grunts of a half meter at once. A grunt spawns at the center of the open cell
    // nearest the point of the box nearest the rally point; a cell is open to it when its center
    // is at least 0.5 m from the box. A tie goes to the lower cell, row by row from (−10, −10).
    let mut shop = Shop::ordering();
    let bounds = Bounds::new([Num::int(-10); 2], [Num::int(10); 2]).unwrap();
    let walker = Walker {
        layer: Layer::FIRST,
        radius: Num::HALF,
    };
    let world = &mut shop.sim.world;
    world.insert_resource(bounds);
    Navigation::load_pathing(
        world,
        Grid::new(Num::ONE, bounds).unwrap(),
        &[],
        vec![walker],
    );
    let mut walkers = ByType::default();
    walkers.set(shop.grunt, walker);
    world.insert_resource(walkers);
    let train = internals::train(world, shop.grunt, Ticks::ZERO, None);
    let player = PlayerSlot::new(0);
    let producer = shop.id();
    shop.producer(producer, 1, 0, Some(player), train, TrainQueue::default());
    let body = BodyForm::boxed([Num::int(4), Num::int(2)])
        .unwrap()
        .at(Num::ZERO);
    shop.sim.insert(producer, body);
    let flag = shop
        .sim
        .spawn(half(0, 12), (Team::new(1), Body::new(Num::HALF).unwrap()));
    let rally = |target| Action::Rally {
        target: Some(target),
    };
    let point = |x: i64, z: i64| RallyTarget::Point {
        x: Num::int(x),
        z: Num::int(z),
    };
    // Each case's rally order applies in Inputs, and the train ordered in Act spawns as the tick
    // ends: the spawn place, and where the grunt walks.
    // - East, (8, 0): the box's nearest point is (2, 0); the cells (2.5, ±0.5) are as near, the
    //   lower first. It walks to (8, 0).
    // - West, (−8, 0): (−2, 0), and (−2.5, −0.5). Past the bounds at x = −40, the point is taken
    //   to x = −10 first, which changes nothing here.
    // - A unit at (0, 6): (0, 1), and (−0.5, 1.5), of the two cells as near in the row. It walks
    //   to where the unit stands.
    // - None: at the producer, and it stands.
    let cases = [
        (Some(point(8, 0)), half(5, -1), Some(half(16, 0))),
        (Some(point(-40, 0)), half(-5, -1), Some(half(-20, 0))),
        (
            Some(RallyTarget::Unit(flag)),
            half(-1, 3),
            Some(half(0, 12)),
        ),
        (None, half(0, 0), None),
    ];
    for (at, (target, place, walks)) in cases.into_iter().enumerate() {
        shop.tick_ordering(0, producer, Action::Rally { target });
        shop.order(producer);
        shop.tick();
        let spawned = shop.spawned.borrow()[at];
        assert_eq!(spawned.at.pos, place, "{target:?}");
        let entity = shop
            .sim
            .world
            .resource::<EntityIndex>()
            .get(spawned.at.id)
            .unwrap();
        let destination = shop.sim.world.get::<Destination>(entity).unwrap().get();
        assert_eq!(destination, walks, "{target:?}");
    }
    // A unit that died rallies nothing: the grunt spawns at its producer, and stands.
    shop.sim.insert(flag, Dead);
    shop.tick_ordering(0, producer, rally(RallyTarget::Unit(flag)));
    shop.order(producer);
    shop.tick();
    let spawned = shop.spawned.borrow()[4];
    assert_eq!(spawned.at.pos, half(0, 0));
}
