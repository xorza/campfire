use super::*;
use crate::production::requirements::Requirements;
use crate::stats::player_modifiers::PlayerModifiers;
use crate::stats::stats_world::StatsWorldCrate;
use crate::units::modifier_id::ModifierId;

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
    let train = Actions::add_train(&mut game, shop.grunt, Ticks::new(2), None);
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
    let producer = Production::allocate_id(&mut game);
    shop.producer(
        &mut game,
        producer,
        1,
        10,
        Some(player),
        train,
        TrainQueue::default(),
    );
    let step = |game: &mut Match, queued: usize, spawned: usize, tick: u64| {
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
    };
    // Tick 0: 0 + 3 of 4 joins. Tick 1: its 3 held in the queue, 6 of 4 does not; the first spawns
    // as the tick ends, and uses its 3 as a unit.
    step(&mut game, 1, 0, 0);
    step(&mut game, 0, 1, 1);
    // A depot gives 4 more, a cap of 8: tick 2, 3 + 3 joins; tick 3, 9 of 8 does not, and the
    // second spawns.
    let first_depot = Production::owned(&mut game, depot, player);
    step(&mut game, 1, 1, 2);
    step(&mut game, 0, 2, 3);
    // The depot dies: a cap of 4 below the 6 used kills nothing, and stops trains; a depot no
    // player owns gives nothing.
    game.insert_parts(first_depot, Dead);
    Units::spawn_of(&mut game, depot, at(0), Team::new(1), ());
    step(&mut game, 0, 2, 4);
    assert!(
        Shop::spawned(&game)
            .iter()
            .all(|spawn| spawn.owner == Some(player))
    );
    // It lives again, and two more come: 16 given, the cap at its most of 10. 6 + 3 joins; then
    // 9 + 3 of 10 does not.
    let entity = game.unit_entity(first_depot);
    game.entity_mut(entity).remove::<Dead>();
    Production::owned(&mut game, depot, player);
    Production::owned(&mut game, depot, player);
    step(&mut game, 1, 2, 5);
    step(&mut game, 0, 3, 6);
    step(&mut game, 0, 3, 7);
}

#[test]
fn a_dead_producers_queue_holds_its_supply_and_its_own_cost_does_not_count() {
    // A barracks that uses 2 and gives 5, whose queue holds two grunts of 3.
    let mut game = shop_match();
    let shop = Shop::place(&mut game);
    let train = Actions::add_train(&mut game, shop.grunt, Ticks::new(9), None);
    Production::count_supply(
        &mut game,
        10,
        &[(shop.grunt, supply(3, 0)), (shop.barracks, supply(2, 5))],
    );
    let costs = game.resource::<SupplyCosts>();
    let queue = queue(train, &[9, 9]);
    let count = |dead| costs.unit(shop.barracks, dead, true, Some(&queue));
    assert_eq!((count(false).used, count(false).given), (8, 5));
    assert_eq!((count(true).used, count(true).given), (6, 0));
    // A site uses its cost, and gives nothing until it completes.
    let site = costs.unit(shop.barracks, false, false, None);
    assert_eq!((site.used, site.given), (2, 0));
}

#[test]
fn a_train_needs_the_units_and_the_player_modifiers_its_requires_names() {
    // The train needs a living forge of its player's, and its player's modifier 0.
    let mut game = shop_match();
    let shop = Shop::place(&mut game);
    let train = Actions::add_train(&mut game, shop.grunt, Ticks::new(5), None);
    let drill = ModifierId::new(0);
    let mut requirements = Requirements::default();
    requirements.push(train, [shop.forge], [drill]);
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
        1,
        10,
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
    // The forge dies: the next is refused, and the queued one stays.
    game.insert_parts(forge, Dead);
    step(&mut game, 1, 4);
}
