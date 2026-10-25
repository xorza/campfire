use campfire_stats::{ModifierData, PlayerModifiers, Stats, StatsWorld};
use campfire_values::PackageIndex;

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
