use campfire_stats::{ModifierData, PlayerModifiers, Stats, StatsWorld, TypeUpgrade, TypeUpgrades};
use campfire_values::{ModifierId, PackageIndex};

use super::*;

#[test]
fn a_research_pays_as_it_joins_gives_its_modifier_as_it_ends_and_never_runs_twice() {
    // A player research of `drill` takes 3 ticks and 40 gold, a producer research of it 2 ticks
    // and nothing. Player 0 holds two barracks of the player research and one of the producer
    // research, player 1 one of the player research; each player holds 100 gold. A research
    // ordered in tick t joins at time t and gives at time t + its ticks, the end of the tick
    // before.
    let mut game = ordering_match();
    let shop = Shop::place(&mut game);
    Stats::load_modifier(
        &mut game,
        PackageIndex::MODE,
        "drill",
        &ModifierData::default(),
        None,
    );
    let drill = Stats::modifier(&game, PackageIndex::MODE, "drill").unwrap();
    let gold = shop.gold;
    let [player, other] = [0, 1].map(PlayerSlot::new);
    Players::spawn(&mut game, 2, 1);
    for slot in [player, other] {
        game.player_resources_mut(slot)
            .unwrap()
            .add(gold, 100)
            .unwrap();
        let entity = game.player_entity(slot).unwrap();
        game.entity_mut(entity).insert(PlayerModifiers::default());
    }
    let cost = ResourceAmount {
        resource: gold,
        amount: 40,
    };
    let by_player = Production::add_research(
        &mut game,
        drill,
        ResearchHolder::Player,
        Ticks::new(3),
        Some(cost),
    );
    let by_producer = Production::add_research(
        &mut game,
        drill,
        ResearchHolder::Producer,
        Ticks::new(2),
        None,
    );
    let [first, second, own, theirs] = [(); 4].map(|()| Production::allocate_id(&mut game));
    for (id, x, owner, research) in [
        (first, 10, player, by_player),
        (second, 20, player, by_player),
        (own, 30, player, by_producer),
        (theirs, 40, other, by_player),
    ] {
        let team = Team::new(u8::try_from(owner.index()).unwrap());
        shop.producer(
            &mut game,
            id,
            x,
            team,
            Some(owner),
            research,
            TrainQueue::default(),
        );
    }
    let queued = |game: &Match, id| game.part::<TrainQueue>(id).entries().len();
    let gold_of = |game: &Match, slot| game.player_resources(slot).unwrap().amount(gold);
    let holds = |game: &mut Match, slot| game.player_modifiers_mut(slot).unwrap().holds(drill);

    // Tick 0: the first pays 40 and joins its queue. Tick 1: its cancel gives the 40 back, and
    // its modifier is queued no more. Tick 2: it joins again.
    Production::order_train(&mut game, first);
    game.step();
    assert_eq!((queued(&game, first), gold_of(&game, player)), (1, 60));
    let cancel = ProductionDirective::CancelTrain { place: 0 };
    Production::tick_ordering(&mut game, 0, first, cancel);
    assert_eq!((queued(&game, first), gold_of(&game, player)), (0, 100));
    Production::order_train(&mut game, first);
    game.step();
    assert_eq!((queued(&game, first), gold_of(&game, player)), (1, 60));
    // Tick 3: the second's research of the same modifier, queued at the first, is refused and
    // pays nothing; player 1's joins, as its player neither holds nor queues it.
    Production::order_train(&mut game, second);
    Production::order_train(&mut game, theirs);
    game.step();
    assert_eq!((queued(&game, second), gold_of(&game, player)), (0, 60));
    assert_eq!((queued(&game, theirs), gold_of(&game, other)), (1, 60));
    assert!(!holds(&mut game, player));
    // Tick 4: the first's ends, at time 5: player 0 holds `drill`, player 1 not yet.
    game.step();
    assert_eq!(queued(&game, first), 0);
    assert!(holds(&mut game, player) && !holds(&mut game, other));
    // Tick 5: player 0 holds it, so the second's is refused again; player 1's ends.
    Production::order_train(&mut game, second);
    game.step();
    assert_eq!((queued(&game, second), gold_of(&game, player)), (0, 60));
    assert!(holds(&mut game, other));

    // Tick 6: the producer research joins, though the producer carries `drill` from no source,
    // as its player's modifier holds on each of its units: the research's is its own, from
    // itself. It gives at time 8, the end of tick 7, and a second order of it is then refused.
    Production::order_train(&mut game, own);
    game.step();
    assert_eq!(queued(&game, own), 1);
    assert_eq!(Stats::carried_modifiers(&game, own), [(drill, None)]);
    game.step();
    assert_eq!(queued(&game, own), 0);
    let both = [(drill, None), (drill, Some(own))];
    assert_eq!(Stats::carried_modifiers(&game, own), both);
    Production::order_train(&mut game, own);
    game.step();
    assert_eq!(queued(&game, own), 0);
    assert_eq!(Stats::carried_modifiers(&game, own), both);
}

#[test]
fn a_types_upgrades_hold_while_its_players_and_its_own_modifiers_meet_them() {
    // The barracks type takes `tough` by `drill` or `walls`, `armored` by both, and `fast` by
    // `drill` unless `walls`. Player 0's first barracks researches `drill` for its player, its
    // second `walls` for itself, each in 1 tick; a research gives as its tick ends, and its
    // grants hold from the next tick's Resolve.
    let mut game = ordering_match();
    let shop = Shop::place(&mut game);
    let modifier = |game: &mut Match, name| {
        Stats::load_modifier(
            game,
            PackageIndex::MODE,
            name,
            &ModifierData::default(),
            None,
        );
        Stats::modifier(game, PackageIndex::MODE, name).unwrap()
    };
    // Modifiers load in the order of their names.
    let [armored, drill, fast, tough, walls] =
        ["armored", "drill", "fast", "tough", "walls"].map(|name| modifier(&mut game, name));
    let upgrade = |triggers: &[ModifierId], all, conflicts: &[ModifierId], modifier| TypeUpgrade {
        triggers: triggers.into(),
        all,
        conflicts: conflicts.into(),
        when: None,
        below: None,
        modifier,
    };
    let upgrades = TypeUpgrades(Box::new([
        upgrade(&[drill, walls], false, &[], tough),
        upgrade(&[drill, walls], true, &[], armored),
        upgrade(&[drill], false, &[walls], fast),
    ]));
    Shop::give(&mut game, shop.barracks, upgrades);
    let player = PlayerSlot::new(0);
    Players::spawn(&mut game, 1, 1);
    let entity = game.player_entity(player).unwrap();
    game.entity_mut(entity).insert(PlayerModifiers::default());
    let by_player = Production::add_research(
        &mut game,
        drill,
        ResearchHolder::Player,
        Ticks::new(1),
        None,
    );
    let by_producer = Production::add_research(
        &mut game,
        walls,
        ResearchHolder::Producer,
        Ticks::new(1),
        None,
    );
    let [first, own, later] = [(); 3].map(|()| Production::allocate_id(&mut game));
    for (id, x, research) in [(first, 10, by_player), (own, 20, by_producer)] {
        shop.producer(
            &mut game,
            id,
            x,
            Team::new(0),
            Some(player),
            research,
            TrainQueue::default(),
        );
    }
    let upgraded = |game: &Match, id| {
        let carried = Stats::carried_modifiers(game, id).into_iter();
        let held = carried.filter(|&(modifier, _)| [tough, armored, fast].contains(&modifier));
        held.map(|(modifier, _)| modifier).collect::<Vec<_>>()
    };

    // No trigger held: no upgrade.
    game.step();
    assert_eq!(upgraded(&game, first), []);
    // `drill`: `tough` and `fast` on both, not `armored`, from the tick after the research's.
    Production::order_train(&mut game, first);
    game.step();
    assert_eq!(upgraded(&game, first), []);
    game.step();
    assert_eq!(upgraded(&game, first), [fast, tough]);
    assert_eq!(upgraded(&game, own), [fast, tough]);
    // A barracks placed after the research takes them at the next Resolve.
    shop.producer(
        &mut game,
        later,
        30,
        Team::new(0),
        Some(player),
        by_player,
        TrainQueue::default(),
    );
    game.step();
    assert_eq!(upgraded(&game, later), [fast, tough]);
    // The second's own `walls`: it takes `armored` and gives up `fast`, its conflict; the
    // others, which hold no `walls`, keep what they held.
    Production::order_train(&mut game, own);
    game.step();
    game.step();
    assert_eq!(upgraded(&game, own), [armored, tough]);
    assert_eq!(upgraded(&game, first), [fast, tough]);
    // The player's `drill` taken away: the first and the later hold nothing, the second `tough`
    // by its `walls` alone.
    game.player_modifiers_mut(player).unwrap().remove(drill);
    game.step();
    assert_eq!(upgraded(&game, first), []);
    assert_eq!(upgraded(&game, later), []);
    assert_eq!(upgraded(&game, own), [tough]);
}

#[test]
fn a_research_is_refused_while_its_player_holds_or_queues_one_its_upgrade_conflicts_with() {
    // The barracks type takes `gunned` by `gun` unless `tower`, as Zero Hour's Overlord takes one
    // of its upgrades. Player 0's first barracks researches `gun`, its second `tower`, each for
    // the player in 2 ticks and 40 gold of its 100.
    let mut game = ordering_match();
    let shop = Shop::place(&mut game);
    let [gun, gunned, tower] = ["gun", "gunned", "tower"].map(|name| {
        let data = ModifierData::default();
        Stats::load_modifier(&mut game, PackageIndex::MODE, name, &data, None);
        Stats::modifier(&game, PackageIndex::MODE, name).unwrap()
    });
    let upgrade = TypeUpgrade {
        triggers: Box::new([gun]),
        all: false,
        conflicts: Box::new([tower]),
        when: None,
        below: None,
        modifier: gunned,
    };
    let upgrades = TypeUpgrades(Box::new([upgrade]));
    Shop::give(&mut game, shop.barracks, upgrades.clone());
    let player = PlayerSlot::new(0);
    Players::spawn(&mut game, 1, 1);
    game.player_resources_mut(player)
        .unwrap()
        .add(shop.gold, 100)
        .unwrap();
    let entity = game.player_entity(player).unwrap();
    game.entity_mut(entity).insert(PlayerModifiers::default());
    let cost = ResourceAmount {
        resource: shop.gold,
        amount: 40,
    };
    let [by_gun, by_tower] = [gun, tower].map(|modifier| {
        let holder = ResearchHolder::Player;
        Production::add_research(&mut game, modifier, holder, Ticks::new(2), Some(cost))
    });
    let own_gun = Production::add_research(
        &mut game,
        gun,
        ResearchHolder::Producer,
        Ticks::new(2),
        Some(cost),
    );
    let [gunner, builder] = [(); 2].map(|()| Production::allocate_id(&mut game));
    for (id, x, research) in [(gunner, 10, by_gun), (builder, 20, by_tower)] {
        let queue = TrainQueue::default();
        shop.producer(
            &mut game,
            id,
            x,
            Team::new(0),
            Some(player),
            research,
            queue,
        );
    }
    let queued = |game: &Match, id| game.part::<TrainQueue>(id).entries().len();
    let gold = |game: &Match| game.player_resources(player).unwrap().amount(shop.gold);
    // Tick 0: `gun` joins and pays 40. Tick 1: `tower`, whose conflict is queued, is refused and
    // pays nothing. Ticks 2 and 3: `gun` held, `tower` is refused again; the gold stays at 60.
    Production::order_train(&mut game, gunner);
    game.step();
    assert_eq!((queued(&game, gunner), gold(&game)), (1, 60));
    Production::order_train(&mut game, builder);
    game.step();
    assert_eq!((queued(&game, builder), gold(&game)), (0, 60));
    game.step();
    assert!(game.player_modifiers_mut(player).unwrap().holds(gun));
    Production::order_train(&mut game, builder);
    game.step();
    assert_eq!((queued(&game, builder), gold(&game)), (0, 60));
    // A research of `gun` for the producer itself is refused while its player holds `tower`, as
    // the upgrade reads the player's modifiers and the unit's alike, so `gunned` would be withheld.
    game.player_modifiers_mut(player).unwrap().add(tower);
    let own_gunner = Production::allocate_id(&mut game);
    let queue = TrainQueue::default();
    shop.producer(
        &mut game,
        own_gunner,
        30,
        Team::new(0),
        Some(player),
        own_gun,
        queue,
    );
    Production::order_train(&mut game, own_gunner);
    game.step();
    assert_eq!((queued(&game, own_gunner), gold(&game)), (0, 60));
    // `gun` triggers the upgrade `tower` withholds, and `tower` withholds the one `gun` triggers;
    // `gunned` takes part in none.
    assert_eq!(upgrades.withholding(gun).collect::<Vec<_>>(), [tower]);
    assert_eq!(upgrades.withheld(gun).count(), 0);
    assert_eq!(upgrades.withheld(tower).collect::<Vec<_>>(), [gun]);
    assert_eq!(upgrades.withholding(tower).count(), 0);
    assert_eq!(
        upgrades
            .withholding(gunned)
            .chain(upgrades.withheld(gunned))
            .count(),
        0
    );
}
