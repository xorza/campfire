use campfire_sim::{DueIndex, MatchRate};
use campfire_units::{Facing, TurnStep};

use super::*;

/// A gunner of 30 damage within 8 m, 2 ticks after an attack's start every 5 ticks, whose
/// weapon, at place 0 of the slot kind 0, turns with a turret of 180° a second and 2° of
/// tolerance when the turret's slot is `slot` and it carries that place, as `turret` makes its
/// data; the target 5 m along −z, 90° from its facing of 0°.
fn turret_fight(
    game: &mut Match,
    slot: u8,
    turret: impl FnOnce(TurretData) -> TurretData,
) -> [StableId; 2] {
    let [gunner, target] = load_types(game, [combatant(100, 8, 2, 5, 30), dummy()]);
    let rate = *game.resource::<MatchRate>();
    let slot_kind = DeclaredName::new("weapon").unwrap();
    let data = turret(TurretData::new(Num::int(180), Num::int(2), slot_kind).unwrap());
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
    let [gunner, target] = turret_fight(&mut game, 0, |data| data);
    game.steps(14);
    let angle = |game: &Match| game.part::<TurretAim>(gunner).angle();
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
        game.resource::<DueIndex<TurretAim>>()
            .due_at(game.unit_entity(gunner))
    };
    assert_eq!(turning(&game), Some(Tick::ZERO));
    game.steps(29);
    assert_eq!((angle(&game), turning(&game)), (Num::ZERO, None));

    // A weapon of another slot kind than the turret's, or at a place of its kind the turret does
    // not carry, starts at once, its turret turning still; one at a place it carries waits.
    for (slot, places, started) in [
        (1, None, Some(Tick::new(0))),
        (0, Some(vec![1]), Some(Tick::new(0))),
        (0, Some(vec![0, 1]), None),
    ] {
        let mut game = fight_match();
        let [gunner, _] = turret_fight(&mut game, slot, |data| match places {
            Some(places) => data.carrying(places).unwrap(),
            None => data,
        });
        game.step();
        assert_eq!(Attack::of(&game, gunner).started(), started, "{slot}");
        assert_eq!(angle(&game), Num::int(6));
    }
    // A place past the last a turret holds is refused.
    let slot_kind = DeclaredName::new("weapon").unwrap();
    let data = TurretData::new(Num::int(180), Num::int(2), slot_kind).unwrap();
    assert!(data.carrying(vec![Turret::MOST_PLACES]).is_none());
}

#[test]
fn a_pitching_turret_holds_its_attack_until_its_pitch_lies_within_its_tolerance() {
    // Its pitch turns 30° a second, 1° a tick, to its fire pitch of 30°: after n ticks it lies at
    // n°, 27° after 27 ticks, its angle at 90° since 15; in tick 27 it reaches 28°, within 2° of
    // 30°, and the attack starts.
    let mut game = fight_match();
    let pitch = TurretPitchData::new(Num::int(30), Some(Num::int(30)), Num::ZERO, Num::ZERO);
    let [gunner, target] = turret_fight(&mut game, 0, |data| data.pitched(pitch.unwrap()));
    let aim = |game: &Match| *game.part::<TurretAim>(gunner);
    game.steps(27);
    assert_eq!(aim(&game), TurretAim::of(Num::int(90), Num::int(27)));
    assert_eq!(Attack::of(&game, gunner).started(), None);
    game.step();
    assert_eq!(aim(&game).pitch(), Num::int(28));
    assert_eq!(Attack::of(&game, gunner).started(), Some(Tick::new(27)));
    // With its target gone, it pitches back at half its rate, 0.5° a tick, as it turns back.
    game.steps(2);
    let entity = game.unit_entity(target);
    game.despawn(entity);
    game.step();
    let half = Num::HALF;
    assert_eq!(aim(&game), TurretAim::of(Num::int(87), Num::int(30) - half));
}

#[test]
fn a_restored_turret_aim_is_within_a_turn_level_bounds_and_of_a_type_with_a_turret() {
    let mut game = fight_match();
    let [gunner, target] = turret_fight(&mut game, 0, |data| data);
    game.step();
    let [gunner, target] = [gunner, target].map(|id| game.unit_entity(id));
    let aim = |angle: i64, pitch: i64| TurretAim::of(Num::int(angle), Num::int(pitch));
    let checks = [
        (aim(359, 90), gunner),
        (aim(359, -90), gunner),
        (aim(359, 91), gunner),
        (aim(359, 0), target),
    ]
    .map(|(aim, entity)| aim.check(&game, entity));
    assert_eq!(checks, [true, true, false, false]);
    let decoded = |bits: i64| {
        let encoded = Binary::encode(&(Num::from_bits(bits), Num::ZERO));
        Binary::decode::<TurretAim>(&encoded)
    };
    let full = 360 << Num::FRAC_BITS;
    let read =
        [0, full - 1, full].map(|bits| decoded(bits).is_ok_and(|aim| aim.check(&game, gunner)));
    assert_eq!(read, [true, true, false]);
}

#[test]
fn an_attacker_of_no_turret_turns_toward_its_target_and_attacks_within_its_aim() {
    // A gunner facing 0° that turns 6° a tick and aims within 2°, its target 5 m along −z, 90°
    // round: after n ticks it faces 6n°, 84° after 14, and 90° in tick 14, where the attack
    // starts. A turret that carries no place of its weapon's leaves the body to aim it alike.
    for turret in [None, Some(vec![1])] {
        let mut game = fight_match();
        let [gunner, target] = load_types(
            &mut game,
            [combatant(100, 8, 2, 5, 30).aiming(Num::int(2)), dummy()],
        );
        Units::give_type(&mut game, gunner, TurnStep::new(Num::int(6)).unwrap());
        if let Some(places) = turret {
            let rate = *game.resource::<MatchRate>();
            let slot_kind = DeclaredName::new("weapon").unwrap();
            let data = TurretData::new(Num::int(180), Num::int(2), slot_kind).unwrap();
            let data = data.carrying(places).unwrap();
            Units::give_type(&mut game, gunner, Turret::of(&data, SlotKind::new(0), rate));
        }
        let gunner = Spawn::of(gunner, (0, 0, 0)).place(&mut game);
        let target = Spawn::of(target, (0, 0, -5))
            .team(Team::new(1))
            .place(&mut game);
        let facing = |game: &Match| game.part::<Facing>(gunner).degrees();
        Combat::attack(&mut game, gunner, target);
        game.steps(14);
        assert_eq!(facing(&game), Num::int(84));
        assert_eq!(Attack::of(&game, gunner).started(), None);
        game.step();
        assert_eq!(facing(&game), Num::int(90));
        assert_eq!(Attack::of(&game, gunner).started(), Some(Tick::new(14)));
    }
}
