use bevy_ecs::system::SystemState;
use campfire_stats::{PlayerModifiers, StatsWorld};
use campfire_values::ModifierId;

use super::*;

/// A supply of `cost` used and `provides` given.
const fn supply(cost: u32, provides: u32) -> SupplyData {
    SupplyData { cost, provides }
}

#[test]
fn a_train_joins_only_with_room_under_its_players_cap_and_holds_its_supply_until_it_spawns() {
    // Player 0's barracks gives 4 of a most of 10; a grunt uses 3, and trains in 2 ticks: one
    // ordered in tick t joins at time t and spawns at time t + 2, the end of tick t + 1.
    let mut game = shop_match();
    let shop = Shop::place(&mut game);
    let train = Production::add_train(&mut game, shop.grunt, Ticks::new(2), None);
    let (grunt, barracks, depot) = (shop.grunt, shop.barracks, shop.depot);
    Production::count_supply(
        &mut game,
        10,
        &[
            (grunt, supply(3, 0)),
            (barracks, supply(0, 4)),
            (depot, supply(0, 4)),
        ],
    );
    let player = PlayerSlot::new(0);
    Players::spawn(&mut game, 1, 0);
    let producer = Production::allocate_id(&mut game);
    shop.producer(
        &mut game,
        producer,
        10,
        Team::new(1),
        Some(player),
        train,
        TrainQueue::default(),
    );
    // Each tick's end leaves the queue's length, the spawns, and the supply the player's entity
    // shows: what is used, and the cap.
    let step = |game: &mut Match, queued: usize, spawned: usize, supply: (u64, u64), tick: u64| {
        Production::order_train(game, producer);
        game.step();
        assert_eq!(
            (
                game.part::<TrainQueue>(producer).entries().len(),
                shop.spawns(game).len()
            ),
            (queued, spawned),
            "tick {tick}"
        );
        let entity = game.player_entity(player).unwrap();
        let shown = game.get::<PlayerSupply>(entity).copied().unwrap();
        assert_eq!((shown.used(), shown.cap()), supply, "tick {tick}");
    };
    // Tick 0: 0 + 3 of 4 joins. Tick 1: its 3 held in the queue, 6 of 4 does not; the first spawns
    // as the tick ends, and uses its 3 as a unit.
    step(&mut game, 1, 0, (3, 4), 0);
    step(&mut game, 0, 1, (3, 4), 1);
    // A depot gives 4 more, a cap of 8: tick 2, 3 + 3 joins; tick 3, 9 of 8 does not, and the
    // second spawns.
    let first_depot = Production::owned(&mut game, depot, player);
    step(&mut game, 1, 1, (6, 8), 2);
    step(&mut game, 0, 2, (6, 8), 3);
    // The depot dies: a cap of 4 below the 6 used kills nothing, and stops trains; a depot no
    // player owns gives nothing.
    game.insert_parts(first_depot, Dead);
    Spawn::of(depot, (0, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
    step(&mut game, 0, 2, (6, 4), 4);
    assert!(
        Shop::spawned(&mut game, shop.grunt)
            .iter()
            .all(|spawn| spawn.owner == Some(player))
    );
    // It lives again, and two more come: 16 given, the cap at its most of 10. 6 + 3 joins; then
    // 9 + 3 of 10 does not.
    let entity = game.unit_entity(first_depot);
    game.entity_mut(entity).remove::<Dead>();
    let second_depot = Production::owned(&mut game, depot, player);
    let third_depot = Production::owned(&mut game, depot, player);
    step(&mut game, 1, 2, (9, 10), 5);
    step(&mut game, 0, 3, (9, 10), 6);
    step(&mut game, 0, 3, (9, 10), 7);
    // Two depots die: 16 - 8 = 8 given, below the 9 used.
    for id in [first_depot, second_depot] {
        game.insert_parts(id, Dead);
    }
    step(&mut game, 0, 3, (9, 8), 8);
    // The third despawns, and a new one spawns in the same tick: 8 - 4 + 4 = 8 given.
    let gone = game.unit_entity(third_depot);
    game.entity_mut(gone).despawn();
    Production::owned(&mut game, depot, player);
    step(&mut game, 0, 3, (9, 8), 9);
}

#[test]
fn a_dead_producers_queue_holds_its_supply_and_its_own_cost_does_not_count() {
    // A barracks that uses 2 and gives 5, whose queue holds two grunts of 3.
    let mut game = shop_match();
    let shop = Shop::place(&mut game);
    let train = Production::add_train(&mut game, shop.grunt, Ticks::new(9), None);
    Production::count_supply(
        &mut game,
        10,
        &[(shop.grunt, supply(3, 0)), (shop.barracks, supply(2, 5))],
    );
    let costs = game.resource::<SupplyCostBook>();
    let queue = queue(train, &[9, 9]);
    let unit = |dead, complete, queue| SupplyUnit {
        unit_type: shop.barracks,
        dead,
        complete,
        queue,
    };
    let count = |dead| costs.unit(unit(dead, true, Some(&queue)));
    assert_eq!((count(false).used, count(false).given), (8, 5));
    assert_eq!((count(true).used, count(true).given), (6, 0));
    // A site uses its cost, and gives nothing until it completes.
    let site = costs.unit(unit(false, false, None));
    assert_eq!((site.used, site.given), (2, 0));
}

#[test]
fn a_train_needs_the_units_and_the_player_modifiers_its_requires_names() {
    // The train needs a living forge or depot of its player's, and its player's modifier 0.
    let mut game = shop_match();
    let shop = Shop::place(&mut game);
    let train = Production::add_train(&mut game, shop.grunt, Ticks::new(5), None);
    let drill = ModifierId::new(0);
    let mut requirements = RequirementBook::default();
    requirements.push(train, [[shop.forge, shop.depot]], [drill]);
    game.insert_resource(requirements);
    // A match of production alone, whose players hold no modifiers until one is given them.
    Players::spawn(&mut game, 2, 0);
    let entity = game.player_entity(PlayerSlot::new(0)).unwrap();
    let given = PlayerModifiers::default();
    game.entity_mut(entity).insert(given);
    let player = PlayerSlot::new(0);
    let producer = Production::allocate_id(&mut game);
    shop.producer(
        &mut game,
        producer,
        10,
        Team::new(1),
        Some(player),
        train,
        TrainQueue::default(),
    );
    let step = |game: &mut Match, queued: usize, tick: u64| {
        Production::order_train(game, producer);
        game.step();
        assert_eq!(
            game.part::<TrainQueue>(producer).entries().len(),
            queued,
            "tick {tick}"
        );
    };
    // Neither: refused. Another player's forge, and a forge but no modifier: refused.
    step(&mut game, 0, 0);
    Production::owned(&mut game, shop.forge, PlayerSlot::new(1));
    step(&mut game, 0, 1);
    let forge = Production::owned(&mut game, shop.forge, player);
    step(&mut game, 0, 2);
    // Both: it joins.
    let held = game.player_modifiers_mut(player);
    held.unwrap().add(drill);
    step(&mut game, 1, 3);
    // The forge dies: the next is refused, and the queued one stays; a depot of its player's
    // meets the entry in its place, and the next joins.
    game.insert_parts(forge, Dead);
    step(&mut game, 1, 4);
    Production::owned(&mut game, shop.depot, player);
    step(&mut game, 2, 5);
}

#[test]
fn a_producer_may_start_a_train_only_as_its_order_would_start_it() {
    // Player 0's barracks trains grunts of 3 supply and 5 gold, in its queue of 3, each needing a
    // forge of its player's; it gives `provides` supply of a most of 20. A tick runs before each
    // read, in which the holdings follow the units. With 10 gold, a forge, an empty queue and 4
    // supply given, it may start one; each case lacks one thing the train needs.
    let scene = |gold: i64, forge: bool, queued: &[u64], provides: u32, dead: bool| {
        let mut game = shop_match();
        let shop = Shop::place(&mut game);
        let cost = ResourceAmount {
            resource: shop.gold,
            amount: 5,
        };
        let train = Production::add_train(&mut game, shop.grunt, Ticks::new(9), Some(cost));
        let mut requirements = RequirementBook::default();
        requirements.push(train, [[shop.forge]], []);
        game.insert_resource(requirements);
        Production::count_supply(
            &mut game,
            20,
            &[
                (shop.grunt, supply(3, 0)),
                (shop.barracks, supply(0, provides)),
            ],
        );
        let player = PlayerSlot::new(0);
        Players::spawn(&mut game, 1, 1);
        let mut held = game.player_resources_mut(player).unwrap();
        held.add(shop.gold, gold).unwrap();
        if forge {
            Production::owned(&mut game, shop.forge, player);
        }
        let producer = Production::allocate_id(&mut game);
        let queue = queue(train, queued);
        shop.producer(
            &mut game,
            producer,
            10,
            Team::new(1),
            Some(player),
            train,
            queue,
        );
        if dead {
            game.insert_parts(producer, Dead);
        }
        game.step();
        let now = game.resource::<SimTick>().end();
        let mut checks = SystemState::<StartChecks<'_, '_>>::new(&mut game);
        checks.get(&game).unwrap().may_start(producer, 0, now)
    };
    assert!(scene(10, true, &[], 4, false));
    for (case, gold, forge, queued, provides, dead) in [
        ("4 gold", 4, true, &[][..], 4, false),
        ("no forge", 10, false, &[], 4, false),
        ("3 + 3 used of 4", 10, true, &[9], 4, false),
        ("a full queue", 10, true, &[9, 9, 9], 20, false),
        ("dead", 10, true, &[], 4, true),
    ] {
        assert!(!scene(gold, forge, queued, provides, dead), "{case}");
    }
    // A queue of two leaves it room, 6 + 3 of 20.
    assert!(scene(10, true, &[9, 9], 20, false));
}
