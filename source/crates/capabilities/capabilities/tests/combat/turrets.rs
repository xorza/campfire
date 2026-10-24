use campfire_sim::{DueIndex, MatchRate};

use super::*;

/// A gunner of 30 damage within 8 m, 2 ticks after an attack's start every 5 ticks, whose
/// weapon, of the slot kind 0, turns with a turret of 180° a second and 2° of tolerance when the
/// turret's slot is `slot`; the target 5 m along −z, 90° from its facing of 0°.
fn turret_fight(game: &mut Match, slot: u8) -> [StableId; 2] {
    let [gunner, target] = load_types(game, [combatant(100, 8, 2, 5, 30), dummy()]);
    let rate = *game.resource::<MatchRate>();
    let slot_kind = DeclaredName::new("weapon").unwrap();
    let data = TurretData::new(Num::int(180), Num::int(2), slot_kind).unwrap();
    Units::give_type(game, gunner, Turret::of(&data, SlotKind::new(slot), rate));
    let gunner = Spawn::of(gunner, (0, 0, 0)).place(game);
    let target = Spawn::of(target, (0, 0, -5)).team(Team::new(1)).place(game);
    Combat::attack(game, gunner, target);
    [gunner, target]
}

#[test]
fn a_turret_turns_toward_its_target_and_its_attack_starts_within_its_tolerance() {
    // At 30 ticks a second, 6° a tick: after tick k the turret lies at 6(k + 1)°, 84° after
    // tick 13, 6° from its aim, and 90° in tick 14, where the attack starts.
    let mut game = fight_match();
    let [gunner, target] = turret_fight(&mut game, 0);
    for _ in 0..14 {
        game.step();
    }
    let angle = |game: &Match| game.part::<TurretAngle>(gunner).degrees();
    assert_eq!(angle(&game), Num::int(84));
    assert_eq!(Attack::of(&game, gunner).started(), None);
    game.step();
    assert_eq!(angle(&game), Num::int(90));
    assert_eq!(Attack::of(&game, gunner).started(), Some(Tick::new(14)));
    // A target that is gone leaves the turret idle: it turns back at half its rate, 3° a tick.
    let entity = game.unit_entity(target);
    game.despawn(entity);
    game.step();
    assert_eq!(angle(&game), Num::int(87));
    // The turret is held while it turns back, 87 ÷ 3 = 29 ticks, and leaves the index at rest.
    let turning = |game: &Match| {
        game.resource::<DueIndex<TurretAngle>>()
            .due_at(game.unit_entity(gunner))
    };
    assert_eq!(turning(&game), Some(Tick::ZERO));
    for _ in 0..29 {
        game.step();
    }
    assert_eq!((angle(&game), turning(&game)), (Num::ZERO, None));

    // A weapon of another slot kind than the turret's starts at once, its turret turning still.
    let mut game = fight_match();
    let [gunner, _] = turret_fight(&mut game, 1);
    game.step();
    assert_eq!(Attack::of(&game, gunner).started(), Some(Tick::new(0)));
    assert_eq!(angle(&game), Num::int(6));
}

#[test]
fn a_restored_turret_angle_is_within_a_turn_and_of_a_type_with_a_turret() {
    let mut game = fight_match();
    let [gunner, target] = turret_fight(&mut game, 0);
    game.step();
    let [gunner, target] = [gunner, target].map(|id| game.unit_entity(id));
    let checks = [
        (TurretAngle::of(Num::int(359)), gunner),
        (TurretAngle::of(Num::int(359)), target),
    ]
    .map(|(angle, entity)| angle.check(&game, entity));
    assert_eq!(checks, [true, false]);
    let decoded = |bits: i64| Binary::decode::<TurretAngle>(&Binary::encode(&Num::from_bits(bits)));
    let full = 360 << Num::FRAC_BITS;
    let read =
        [0, full - 1, full].map(|bits| decoded(bits).is_ok_and(|angle| angle.check(&game, gunner)));
    assert_eq!(read, [true, true, false]);
}
