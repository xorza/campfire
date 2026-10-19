use super::*;

/// The types of a gunner of 30 damage within 8 m, 2 ticks after an attack's start every 5 ticks,
/// its hit flying at `speed` m/s, and of a target.
fn slow_types(game: &mut Match, speed: i64) -> [UnitType; 2] {
    let gunner = combatant(100, 8, 2, 5, 30).slow(Num::int(speed));
    arm(game, [gunner, dummy()])
}

/// The gunner at the origin attacking the target 6 m along `x`.
fn slow_fight(game: &mut Match, speed: i64) -> [StableId; 2] {
    let [gunner, target] = slow_types(game, speed);
    let gunner = Mode::spawn(game, gunner, (0, 0, 0), Team::new(0), ());
    let target = Mode::spawn(game, target, (6, 0, 0), Team::new(1), ());
    Combat::attack(game, gunner, target);
    [gunner, target]
}

#[test]
fn a_slow_weapons_hit_lands_its_distance_over_its_speed_after_its_shot_wherever_it_stands() {
    // 60 m/s at 30 Hz, 2 m a tick: the shot in tick 2 lands ⌈6 ÷ 2⌉ = 3 ticks later, in tick 5,
    // on the target where it then stands, though it moved 4 m away.
    let mut game = fight_match();
    let [_, target] = slow_fight(&mut game, 60);
    game.run_until(5);
    assert_eq!(Stats::health(&game, target), 100);
    let entity = game.unit_entity(target);
    *game.get_mut::<Position>(entity).unwrap() = Position::from(Vec3::from((6, 0, 4)));
    game.step();
    assert_eq!(Stats::health(&game, target), 70);
    // Its next attack, from tick 5, shoots in tick 7, √(6² + 4²) ≈ 7.2 m away, 4 ticks: tick 11.
    game.run_until(11);
    assert_eq!(Stats::health(&game, target), 70);
    game.step();
    assert_eq!(Stats::health(&game, target), 40);

    // A target that a tag keeps from being a target after the shot still takes the hit on its way.
    let mut game = fight_match();
    let [_, target] = slow_fight(&mut game, 60);
    game.run_until(3);
    Units::set_blocks(&mut game, target, &[Block::Target]);
    game.run_until(6);
    assert_eq!(Stats::health(&game, target), 70);

    // 300 m/s, 10 m a tick: 6 m is less than a tick, and the hit lands in its shot's tick.
    let mut game = fight_match();
    let [_, target] = slow_fight(&mut game, 300);
    game.run_until(3);
    assert_eq!(Stats::health(&game, target), 70);
}

#[test]
fn a_held_hit_is_state_and_one_on_a_target_gone_by_then_lands_nothing() {
    // The shot in tick 2 is held until tick 5: a snapshot holds it, and the hit lands in the
    // restored match as in the first. A target gone by then takes nothing, and the hit goes.
    let mut game = fight_match();
    let [_, target] = slow_fight(&mut game, 60);
    game.run_until(3);
    let mut snapshot = Vec::new();
    game.snapshot(&mut snapshot);
    assert_eq!(Combat::held_hits(&game), 1);
    // A fresh match of the same types, emptied of the entities it starts with.
    let mut restored = fight_match();
    slow_types(&mut restored, 60);
    let held: Vec<_> = restored
        .resource::<EntityIndex>()
        .iter()
        .map(|(_, entity)| entity)
        .collect();
    for entity in held {
        restored.despawn(entity);
    }
    restored.restore(&snapshot).unwrap();
    restored.run_until(6);
    assert_eq!(Stats::health(&restored, target), 70);
    let entity = game.unit_entity(target);
    game.despawn(entity);
    game.run_until(6);
    assert_eq!(Combat::held_hits(&game), 0);

    // A target that dies before its hit lands loses the hit as it dies, in tick 3, so a respawn
    // gets none from before.
    let mut game = fight_match();
    let [_, target] = slow_fight(&mut game, 60);
    game.run_until(3);
    assert_eq!(Combat::held_hits(&game), 1);
    let entity = game.unit_entity(target);
    let mut life = game.get_mut::<Life>(entity).unwrap();
    let all = life.current();
    life.take(all);
    game.step();
    assert_eq!(Combat::held_hits(&game), 0);
}
