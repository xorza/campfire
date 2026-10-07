use super::*;
use crate::production::requirements::Requirements;
use crate::stats::player_modifiers::{PlayerModifier, PlayerModifiers};
use crate::units::modifier_id::ModifierId;

/// A supply of `cost` used and `provides` given.
const fn supply(cost: u32, provides: u32) -> SupplyData {
    SupplyData { cost, provides }
}

#[test]
fn a_train_joins_only_with_room_under_its_players_cap_and_holds_its_supply_until_it_spawns() {
    // Player 0's barracks gives 4 of a most of 10; a grunt uses 3, and trains in 2 ticks: one
    // ordered in tick t joins at time t and spawns at time t + 2, the end of tick t + 1.
    let mut shop = Shop::new();
    let train = internals::train(&mut shop.sim.world, shop.grunt, Ticks::new(2), None);
    let (grunt, barracks, depot) = (shop.grunt, shop.barracks, shop.depot);
    shop.count_supply(
        10,
        &[
            (grunt, supply(3, 0)),
            (barracks, supply(0, 4)),
            (depot, supply(0, 4)),
        ],
    );
    let player = PlayerSlot::new(0);
    let producer = shop.id();
    shop.producer(producer, 1, 10, Some(player), train, TrainQueue::default());
    let step = |shop: &mut Shop, queued: usize, spawned: usize, tick: u64| {
        shop.order(producer);
        shop.tick();
        assert_eq!(
            (shop.queued(producer), shop.spawns().len()),
            (queued, spawned),
            "tick {tick}"
        );
    };
    // Tick 0: 0 + 3 of 4 joins. Tick 1: its 3 held in the queue, 6 of 4 does not; the first spawns
    // as the tick ends, and uses its 3 as a unit.
    step(&mut shop, 1, 0, 0);
    step(&mut shop, 0, 1, 1);
    // A depot gives 4 more, a cap of 8: tick 2, 3 + 3 joins; tick 3, 9 of 8 does not, and the
    // second spawns.
    let first_depot = shop.owned(depot, player);
    step(&mut shop, 1, 1, 2);
    step(&mut shop, 0, 2, 3);
    // The depot dies: a cap of 4 below the 6 used kills nothing, and stops trains; a depot no
    // player owns gives nothing.
    shop.sim.insert(first_depot, Dead);
    shop.sim.spawn(at(0), (depot, Team::new(1)));
    step(&mut shop, 0, 2, 4);
    assert!(
        shop.spawned
            .borrow()
            .iter()
            .all(|spawn| spawn.owner == Some(player))
    );
    // It lives again, and two more come: 16 given, the cap at its most of 10. 6 + 3 joins; then
    // 9 + 3 of 10 does not.
    let entity = shop.sim.entity(first_depot);
    shop.sim.world.entity_mut(entity).remove::<Dead>();
    shop.owned(depot, player);
    shop.owned(depot, player);
    step(&mut shop, 1, 2, 5);
    step(&mut shop, 0, 3, 6);
    step(&mut shop, 0, 3, 7);
}

#[test]
fn a_dead_producers_queue_holds_its_supply_and_its_own_cost_does_not_count() {
    // A barracks that uses 2 and gives 5, whose queue holds two grunts of 3.
    let mut shop = Shop::new();
    let train = internals::train(&mut shop.sim.world, shop.grunt, Ticks::new(9), None);
    shop.count_supply(
        10,
        &[(shop.grunt, supply(3, 0)), (shop.barracks, supply(2, 5))],
    );
    let costs = shop.sim.world.resource::<SupplyCosts>();
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
    let mut shop = Shop::new();
    let train = internals::train(&mut shop.sim.world, shop.grunt, Ticks::new(5), None);
    let drill = ModifierId::nth(0);
    let mut requirements = Requirements::default();
    requirements.push(train, [shop.forge], [drill]);
    shop.sim.world.insert_resource(requirements);
    shop.sim.world.insert_resource(PlayerModifiers::default());
    let player = PlayerSlot::new(0);
    let producer = shop.id();
    shop.producer(producer, 1, 10, Some(player), train, TrainQueue::default());
    let step = |shop: &mut Shop, queued: usize, tick: u64| {
        shop.order(producer);
        shop.tick();
        assert_eq!(shop.queued(producer), queued, "tick {tick}");
    };
    // Neither: refused. Another player's forge, and a forge but no modifier: refused.
    step(&mut shop, 0, 0);
    shop.owned(shop.forge, PlayerSlot::new(1));
    step(&mut shop, 0, 1);
    let forge = shop.owned(shop.forge, player);
    step(&mut shop, 0, 2);
    // Both: it joins.
    let held = PlayerModifier {
        player,
        modifier: drill,
    };
    shop.sim.world.resource_mut::<PlayerModifiers>().add(held);
    step(&mut shop, 1, 3);
    // The forge dies: the next is refused, and the queued one stays.
    shop.sim.insert(forge, Dead);
    step(&mut shop, 1, 4);
}
