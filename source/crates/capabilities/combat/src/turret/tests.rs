use std::num::NonZeroU32;

use campfire_common::DeclaredName;
use campfire_units::Facing;

use super::*;

fn data(turn_rate: i64, tolerance: i64) -> TurretData {
    TurretData::new(
        Num::int(turn_rate),
        Num::int(tolerance),
        DeclaredName::new("weapon").unwrap(),
    )
    .unwrap()
}

fn rate() -> MatchRate {
    MatchRate::new(NonZeroU32::new(30).unwrap())
}

/// A turret of 180° a second and 2° of tolerance at 30 ticks a second, 6° a tick, which pitches
/// 60° a second, 2° a tick, from a least of −20° up, back to a rest of 10°, at `fire` when given.
fn pitched(fire: Option<i64>) -> Turret {
    let pitch = TurretPitchData::new(
        Num::int(60),
        fire.map(Num::int),
        Num::int(-20),
        Num::int(10),
    )
    .unwrap();
    Turret::of(&data(180, 2).pitched(pitch), SlotKind::new(0), rate())
}

#[test]
fn a_turret_turns_its_rate_a_tick_and_back_at_half_of_it() {
    // 180° a second at 30 ticks a second: 6° a tick toward a target, 3° back to rest.
    let turret = Turret::of(&data(180, 2), SlotKind::new(0), rate());
    assert_eq!(
        [turret.turn, turret.back].map(TurnStep::get),
        [Num::int(6), Num::int(3)]
    );
    // A rate below a bit a tick still turns, by a bit.
    let slow = Turret::of(
        &TurretData {
            turn_rate: Num::EPSILON,
            ..data(1, 2)
        },
        SlotKind::new(0),
        rate(),
    );
    assert_eq!(slow.turn.get(), Num::EPSILON);
    // A pitch of 60° a second: 2° a tick toward its aim, 1° back to its rest.
    let pitch = pitched(None).pitch.unwrap();
    assert_eq!(
        [pitch.turn, pitch.back].map(TurnStep::get),
        [Num::int(2), Num::int(1)]
    );
}

#[test]
fn a_turrets_angle_points_against_the_body_and_turns_the_shorter_way() {
    // A unit at the origin facing 90°, toward −z: a target along `x` lies at 0°, so at −90°,
    // 270°, against the body; one along −z at 0°. A turret that does not pitch aims level.
    let turret = Turret::of(&data(180, 2), SlotKind::new(0), rate());
    let facing = Facing::of(Num::int(90));
    let toward = |x: i32, y: i32, z: i32| {
        TurretAim::toward(&turret, facing, Vec3::ZERO, Vec3::from((x, y, z)))
    };
    let aim = |angle: i64| TurretAim::of(Num::int(angle), Num::ZERO);
    assert_eq!(toward(5, 0, 0), Some(aim(270)));
    assert_eq!(toward(0, 9, -5), Some(aim(0)));
    assert_eq!(toward(0, 5, 0), None);
    // From 0° toward 270°, the shorter way is back, 6° a tick to 354°; within 6° of 270°, it
    // reaches it.
    let rest = TurretAim::rest(&turret);
    assert_eq!(rest, aim(0));
    assert_eq!(rest.aimed(aim(270), &turret), aim(354));
    assert_eq!(aim(275).aimed(aim(270), &turret), aim(270));
    // Back to rest at 3° a tick.
    assert_eq!(aim(270).rested(&turret), aim(273));
    // Within 2° of an aim either way, across 0° too; 2° and a bit is past it.
    let two = Num::int(2);
    let near = |angle: i64, toward: i64| aim(angle).within(aim(toward), two);
    assert!(near(359, 1) && near(1, 359) && near(10, 12));
    assert!(!near(10, 13));
    let past = TurretAim::of(Num::int(12) + Num::EPSILON, Num::ZERO);
    assert!(!aim(10).within(past, two));
}

#[test]
fn a_turret_pitches_to_its_fire_pitch_or_toward_its_target_no_lower_than_its_least() {
    // Its top 4 m up, a target 3 m along `x` at the ground: 4 down over 3 along, the pitch of
    // atan2(−4, 3) = −53.13°, below the least of −20°; 4 up over 3, 53.13°; with a fire pitch of
    // 70°, 70° whatever the way.
    let facing = Facing::of(Num::ZERO);
    let top = Vec3::from((0, 4, 0));
    let toward = |turret: &Turret, y: i32| {
        TurretAim::toward(turret, facing, top, Vec3::from((3, y, 0)))
            .unwrap()
            .pitch()
    };
    let free = pitched(None);
    assert_eq!(toward(&free, 0), Num::int(-20));
    let up = toward(&free, 8);
    let near: Num = "53.130102".parse().unwrap();
    let slack: Num = "0.00001".parse().unwrap();
    assert!(near - slack < up && up < near + slack);
    let fixed = pitched(Some(70));
    assert_eq!([toward(&fixed, 0), toward(&fixed, 8)], [Num::int(70); 2]);
    // Its rest is 10°; from there to 70° at 2° a tick, 12° after one, and 70° after 30; back
    // at 1° a tick.
    let rest = TurretAim::rest(&fixed);
    assert_eq!(rest, TurretAim::of(Num::ZERO, Num::int(10)));
    let aim = TurretAim::of(Num::ZERO, Num::int(70));
    let once = rest.aimed(aim, &fixed);
    assert_eq!(once.pitch(), Num::int(12));
    let all = (1..30).fold(once, |turned, _| turned.aimed(aim, &fixed));
    assert_eq!(all, aim);
    assert_eq!(all.rested(&fixed).pitch(), Num::int(69));
    // An aim of the right bearing 3° short of its pitch is past a tolerance of 2°; 2° short
    // is within it.
    let short = |by: i64| TurretAim::of(Num::ZERO, Num::int(70 - by));
    assert_eq!(
        [
            short(3).within(aim, Num::int(2)),
            short(2).within(aim, Num::int(2))
        ],
        [false, true]
    );
}
