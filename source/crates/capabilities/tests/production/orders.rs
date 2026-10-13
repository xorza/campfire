use campfire_core::{Layer, MapBook, TeamSet};
use campfire_geometry::{Bounds, Grid};
use campfire_navigation::{Navigation, Walker};
use campfire_vision::{SeenBy, Vision};

use super::*;

#[test]
fn a_cancel_refunds_what_its_entry_paid_and_a_cancelled_head_starts_the_next() {
    // Player 0 holds 0 gold; its barracks queues three trains of 4 ticks, pushed at tick 0, that
    // paid 10, 20 and 30 gold: the head is done at time 4.
    let mut game = ordering_match();
    let shop = Shop::place(&mut game);
    let gold = ResourceId::nth(0);
    Players::spawn(&mut game, 1, 1);
    let train = Actions::add_train(&mut game, shop.grunt, Ticks::new(4), None);
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
    let producer = Production::allocate_id(&mut game);
    shop.producer(
        &mut game,
        producer,
        10,
        Team::new(1),
        Some(player),
        train,
        queue,
    );
    let held = |game: &mut Match| {
        let amount = game.player_resources(player).unwrap().amount(gold);
        (
            amount,
            game.part::<TrainQueue>(producer).entries().len(),
            shop.spawns(game).len(),
        )
    };
    let cancel = |place| ProductionAction::CancelTrain { place };
    // Tick 0: the entry behind the head gives back its 20; the head keeps its time. Another
    // player's cancel, and one past the queue's end, do nothing.
    Production::tick_ordering(&mut game, 0, producer, cancel(1));
    assert_eq!(held(&mut game), (20, 2, 0));
    Production::tick_ordering(&mut game, 1, producer, cancel(0));
    Production::tick_ordering(&mut game, 0, producer, cancel(2));
    assert_eq!(held(&mut game), (20, 2, 0));
    // Tick 3: the head gives back its 10, and the last starts at time 3, to be done at time 7,
    // the end of tick 6, not at time 4.
    Production::tick_ordering(&mut game, 0, producer, cancel(0));
    assert_eq!(held(&mut game), (30, 1, 0));
    for tick in 4..=6 {
        assert_eq!(held(&mut game).2, 0, "tick {tick}");
        game.step();
    }
    assert_eq!(held(&mut game), (30, 0, 1));

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
    game.insert_parts(producer, queue);
    let mut resources = game.player_resources_mut(player).unwrap();
    resources.add(gold, i64::MAX - 35).unwrap();
    Production::tick_ordering(&mut game, 0, producer, cancel(0));
    assert_eq!(held(&mut game), (i64::MAX - 5, 1, 1));
}

#[test]
fn a_trained_unit_spawns_on_its_rally_points_side_of_its_producer_and_goes_there() {
    // Over 1 m cells from (−10, −10) to (10, 10), a barracks of a box 4 by 2 m at the origin
    // trains grunts of a half meter at once. A grunt spawns at the center of the open cell
    // nearest the point of the box nearest the rally point; a cell is open to it when its center
    // is at least 0.5 m from the box. A tie goes to the lower cell, row by row from (−10, −10).
    let mut game = ordering_match();
    let shop = Shop::place(&mut game);
    let bounds = Bounds::new([Num::int(-10); 2], [Num::int(10); 2]).unwrap();
    let walker = Walker {
        layer: Layer::FIRST,
        radius: Num::HALF,
    };
    game.resource_mut::<MapBook>().bounds = bounds;
    Navigation::load_pathing(&mut game, Grid::new(Num::ONE, bounds).unwrap(), [walker]);
    let mut walkers = ByType::default();
    walkers.set(shop.grunt, walker);
    game.insert_resource(walkers);
    let train = Actions::add_train(&mut game, shop.grunt, Ticks::ZERO, None);
    let player = PlayerSlot::new(0);
    let producer = Production::allocate_id(&mut game);
    shop.producer(
        &mut game,
        producer,
        0,
        Team::new(1),
        Some(player),
        train,
        TrainQueue::default(),
    );
    let body = BodyForm::box_sized([Num::int(4), Num::int(2)])
        .unwrap()
        .at(Num::ZERO);
    game.insert_parts(producer, body);
    let half = BodyForm::circle(Num::HALF).unwrap();
    let flag = Navigation::spawn_unit(
        &mut game,
        halves(0, 12),
        Team::new(1),
        None,
        None,
        Some(half),
    );
    let rally = |target| ProductionAction::Rally {
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
        (Some(point(8, 0)), halves(5, -1), Some(halves(16, 0))),
        (Some(point(-40, 0)), halves(-5, -1), Some(halves(-20, 0))),
        (
            Some(RallyTarget::Unit(flag)),
            halves(-1, 3),
            Some(halves(0, 12)),
        ),
        (None, halves(0, 0), None),
    ];
    for (at, (target, place, walks)) in cases.into_iter().enumerate() {
        Production::tick_ordering(&mut game, 0, producer, ProductionAction::Rally { target });
        Production::order_train(&mut game, producer);
        game.step();
        let spawned = Shop::spawned(&mut game, shop.grunt)[at];
        assert_eq!(spawned.pos, place, "{target:?}");
        let entity = game.entity_of(spawned.id).unwrap();
        let destination = game.get::<Destination>(entity).unwrap().get();
        assert_eq!(destination, walks, "{target:?}");
    }
    // A unit that died rallies nothing: the grunt spawns at its producer, and stands.
    game.insert_parts(flag, Dead);
    Production::tick_ordering(&mut game, 0, producer, rally(RallyTarget::Unit(flag)));
    Production::order_train(&mut game, producer);
    game.step();
    let spawned = Shop::spawned(&mut game, shop.grunt)[4];
    assert_eq!(spawned.pos, halves(0, 0));

    // With a vision grid, a rally to a unit of team 0 at (0, 6) that the producer's team 1 does
    // not see is ignored: the dead flag stays its rally, and the grunt spawns at its producer.
    // Once team 1 sees it, the rally takes it, as the living flag's did.
    Vision::fog_grid(&mut game, 2);
    let watch = Navigation::spawn_unit(
        &mut game,
        halves(0, 12),
        Team::new(0),
        None,
        None,
        Some(half),
    );
    let team0 = TeamSet::of(Team::new(0));
    for (at, (seers, place)) in [
        (team0, halves(0, 0)),
        (team0.with(Team::new(1)), halves(-1, 3)),
    ]
    .into_iter()
    .enumerate()
    {
        game.insert_parts(watch, SeenBy::new(seers));
        Production::tick_ordering(&mut game, 0, producer, rally(RallyTarget::Unit(watch)));
        Production::order_train(&mut game, producer);
        game.step();
        let spawned = Shop::spawned(&mut game, shop.grunt)[5 + at];
        assert_eq!(spawned.pos, place, "{seers:?}");
    }
}

#[test]
fn a_predicting_clients_train_reads_only_what_it_holds() {
    // The client predicts player 0's barracks, which rallies to a unit of team 1 it holds as the
    // server sent it, and trains grunts at once.
    Release::sync_held(&ORDERING, 6, |game| {
        let shop = Shop::place(game);
        let train = Actions::add_train(game, shop.grunt, Ticks::ZERO, None);
        let producer = Production::allocate_id(game);
        let player = PlayerSlot::new(0);
        let queue = TrainQueue::default();
        shop.producer(game, producer, 0, Team::new(0), Some(player), train, queue);
        let form = BodyForm::circle(Num::HALF).unwrap();
        let flag =
            Navigation::spawn_unit(game, halves(0, 12), Team::new(1), None, None, Some(form));
        game.insert_parts(flag, Unpredicted);
        let rally = ProductionAction::Rally {
            target: Some(RallyTarget::Unit(flag)),
        };
        Production::tick_ordering(game, 0, producer, rally);
        Production::order_train(game, producer);
    });
}
