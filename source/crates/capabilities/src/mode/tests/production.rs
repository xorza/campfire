use super::*;

#[test]
fn a_train_pays_at_once_joins_the_queue_and_spawns_its_unit_when_its_time_ends() {
    let picker = r"
fn on_mode_input(ctx, player, name, value) {
    pick(ctx, player, value);
}
";
    let mut game = Game::new(picker, ScriptLimits::ROOMY);
    game.tick(&[(0, input("hero", "hero-x"))]);
    // A grunt for 30 mana and 5 gold, in 300 ms, 3 ticks at 10 a second.
    let int = |value| Ranked::One(Number::Value(Scalar::Int(value)));
    let cost = ["mana", "gold"].map(|name| DeclaredName::new(name).unwrap());
    let train = ActionData {
        kind: ActionKind::Train,
        cost: [(cost[0].clone(), int(30)), (cost[1].clone(), int(5))].into(),
        windup_ms: Some(int(300)),
        unit_type: Some(DeclaredName::new("grunt").unwrap()),
        ..blink_data()
    };
    let world = &mut game.sim.world;
    let train = Actions::load(world, 0, "train_grunt", &train, None, 1).unwrap();
    // Hero X becomes a producer of a queue of 2, with 100 mana, and its player holds 15 gold.
    let mut owned = world.query_filtered::<Entity, With<Owner>>();
    let producer = owned.single(world).unwrap();
    let pools = Pools::new([(PoolId::FIRST, num(10)), (MANA, num(100))]).unwrap();
    let slots = ActionSlots::new([(train, SlotKind::new(0), 1)]);
    let hero_type = *world.get::<UnitType>(producer).unwrap();
    let production = ProductionData {
        queue: NonZeroU8::new(2).unwrap(),
    };
    let mut producers = world.resource_mut::<ByType<ProductionData>>();
    producers.set(hero_type, production);
    let parts = (pools, slots, TrainQueue::default());
    world.entity_mut(producer).insert(parts);
    let gold = resource("gold").unwrap();
    let player = PlayerSlot::new(0);
    let mut resources = world.resource_mut::<PlayerResources>();
    resources.add(player, gold, 15).unwrap();
    let order = |game: &mut Game| {
        let mut slots = game.sim.world.get_mut::<ActionSlots>(producer).unwrap();
        slots.order(0, ActionTarget::None);
    };
    let paid = |game: &Game| {
        let mana = game
            .sim
            .world
            .get::<Pools>(producer)
            .unwrap()
            .current(MANA)
            .unwrap();
        let held = game
            .sim
            .world
            .resource::<PlayerResources>()
            .amount(player, gold);
        (mana.round(), held)
    };
    let mut grunts = game
        .sim
        .world
        .query_filtered::<(&UnitType, &Owner, &Position), With<Owner>>();
    let hero = *game.sim.world.get::<UnitType>(producer).unwrap();
    let mut trained = |game: &mut Game| {
        let units = grunts.iter(&game.sim.world);
        units.filter(|&(&unit_type, ..)| unit_type != hero).count()
    };

    // The pick ran tick 0. Ticks 1 and 2 each order a train: each pays 30 mana and 5 gold at once.
    // The first's 3 ticks run from time 1 to time 4, the end of tick 3, so its unit spawns in tick
    // 3's Mode stage; the second's run from then to time 7, tick 6. A third, ordered in tick 3's
    // Act stage, before the first spawns, finds the queue full: nothing is paid.
    let steps = [
        (true, (70, 10), 0),
        (true, (40, 5), 0),
        (true, (40, 5), 1),
        (false, (40, 5), 1),
        // Room again: a third pays, and runs from time 7 to time 10, the end of tick 9.
        (true, (10, 0), 1),
        (false, (10, 0), 2),
        (false, (10, 0), 2),
        // Room again, and 10 mana of 30: the order fails, and nothing is paid.
        (true, (10, 0), 2),
        (false, (10, 0), 3),
        (false, (10, 0), 3),
    ];
    for (at, (ordered, expected, made)) in (1..).zip(steps) {
        assert_eq!(game.sim.world.resource::<SimTick>().start().get(), at);
        if ordered {
            order(&mut game);
        }
        game.tick(&[]);
        assert_eq!(paid(&game), expected, "tick {at}");
        assert_eq!(trained(&mut game), made, "tick {at}");
    }
    // Each grunt stands where the producer stood, its player's, and the queue is empty.
    let at = *game.sim.world.get::<Position>(producer).unwrap();
    let made: Vec<_> = grunts
        .iter(&game.sim.world)
        .filter(|&(&unit_type, ..)| unit_type != hero)
        .map(|(_, owner, &pos)| (owner.slot(), pos))
        .collect();
    assert_eq!(made, [(player, at); 3]);
    assert!(
        game.sim
            .world
            .get::<TrainQueue>(producer)
            .unwrap()
            .entries()
            .is_empty()
    );
    assert_eq!(game.failures(), []);
}
