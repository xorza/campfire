use std::num::NonZeroU32;

use campfire_common::DeclaredName;
use campfire_math::Vec3;
use campfire_units::Facing;

use super::*;
use crate::turret::turret_angle::TurretAngle;

fn data(turn_rate: i64, tolerance: i64) -> TurretData {
    TurretData {
        turn_rate: Num::int(turn_rate),
        tolerance_deg: Num::int(tolerance),
        slot: DeclaredName::new("weapon").unwrap(),
    }
}

#[test]
fn a_turret_turns_its_rate_a_tick_and_back_at_half_of_it() {
    // 180° a second at 30 ticks a second: 6° a tick toward a target, 3° back to rest.
    let rate = TickRate::new(NonZeroU32::new(30).unwrap());
    let turret = Turret::of(&data(180, 2), SlotKind::new(0), rate);
    assert_eq!(
        [turret.turn, turret.rest].map(TurnStep::get),
        [Num::int(6), Num::int(3)]
    );
    // A rate below a bit a tick still turns, by a bit.
    let slow = Turret::of(
        &TurretData {
            turn_rate: Num::EPSILON,
            ..data(1, 2)
        },
        SlotKind::new(0),
        rate,
    );
    assert_eq!(slow.turn.get(), Num::EPSILON);
}

#[test]
fn a_turrets_angle_points_against_the_body_and_turns_the_shorter_way() {
    // A unit at the origin facing 90°, toward −z: a target along `x` lies at 0°, so at −90°,
    // 270°, against the body; one along −z at 0°.
    let facing = Facing::of(Num::int(90));
    let toward = |x: i32, z: i32| TurretAngle::toward(facing, Vec3::ZERO, Vec3::from((x, 0, z)));
    assert_eq!(toward(5, 0), Some(TurretAngle::of(Num::int(270))));
    assert_eq!(toward(0, -5), Some(TurretAngle::REST));
    assert_eq!(toward(0, 0), None);
    // From 0° toward 270°, the shorter way is back, 6° a tick to 354°; within 6° of 270°, it
    // reaches it.
    let six = TurnStep::new(Num::int(6)).unwrap();
    let aim = TurretAngle::of(Num::int(270));
    assert_eq!(
        TurretAngle::REST.turned(aim, six),
        TurretAngle::of(Num::int(354))
    );
    assert_eq!(TurretAngle::of(Num::int(275)).turned(aim, six), aim);
    // Within 2° of an aim either way, across 0° too; 2° and a bit is past it.
    let two = Num::int(2);
    let near = |angle: i64, aim: i64| {
        TurretAngle::of(Num::int(angle)).within(TurretAngle::of(Num::int(aim)), two)
    };
    assert!(near(359, 1) && near(1, 359) && near(10, 12));
    assert!(!near(10, 13));
    let past = TurretAngle::of(Num::int(12) + Num::EPSILON);
    assert!(!TurretAngle::of(Num::int(10)).within(past, two));
}
