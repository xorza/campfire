use super::*;

#[test]
fn a_facing_is_its_angle_modulo_360() {
    let degrees = |angle: i32| Facing::of(Num::int(angle.into())).degrees();
    assert_eq!(degrees(0), Num::ZERO);
    assert_eq!(degrees(90), Num::int(90));
    assert_eq!(degrees(360), Num::ZERO);
    assert_eq!(degrees(-90), Num::int(270));
    assert_eq!(degrees(725), Num::int(5));
    // The least step below 0 is the greatest below 360.
    let least = Facing::of(Num::from_bits(-1)).degrees();
    assert_eq!(least.to_bits(), FULL - 1);
    let world = World::new();
    let entity = Entity::PLACEHOLDER;
    assert!(Facing::of(Num::from_bits(-1)).check(&world, entity));
    assert!(!Facing(Num::int(360)).check(&world, entity));
    assert!(!Facing(Num::from_bits(-1)).check(&world, entity));
}

#[test]
fn a_way_gives_its_angle_and_a_turn_takes_the_shorter_side_at_most_its_step() {
    let degrees = |way: [i64; 2]| Facing::along(way.map(Num::int)).map(Facing::degrees);
    // Along `x` is 0°, along `−z` 90°, along `−x` 180° and along `+z` 270°; no way, none.
    assert_eq!(degrees([1, 0]), Some(Num::ZERO));
    assert_eq!(degrees([0, -1]), Some(Num::int(90)));
    assert_eq!(degrees([-3, 0]), Some(Num::int(180)));
    assert_eq!(degrees([0, 5]), Some(Num::int(270)));
    assert_eq!(degrees([0, 0]), None);
    // Toward `x` and `−z` alike: 45°.
    assert_eq!(degrees([2, -2]), Some(Num::int(45)));
    // Apart the shorter way round, across 0° too, and at most half a turn.
    let apart = |a: i64, b: i64| Facing::of(Num::int(a)).apart(Facing::of(Num::int(b)));
    assert_eq!(
        [
            apart(10, 12),
            apart(12, 10),
            apart(359, 1),
            apart(0, 180),
            apart(0, 270)
        ],
        [2, 2, 2, 180, 90].map(Num::int)
    );
    // The turn to a way is signed, counter-clockwise positive, and half a turn is positive.
    let turn = |a: i64, b: i64| Facing::of(Num::int(a)).turn_to(Facing::of(Num::int(b)));
    assert_eq!(
        [
            turn(10, 12),
            turn(12, 10),
            turn(359, 1),
            turn(1, 359),
            turn(0, 180),
            turn(0, 270)
        ],
        [2, -2, 2, -2, 180, -90].map(Num::int)
    );
    let facing = |angle: i64| Facing::of(Num::int(angle));
    let three = TurnStep::new(Num::int(3));
    let toward = |from: i64, way: i64| facing(from).toward(facing(way), three);
    // 3° a step: from 0° toward 90°, to 3°, 87° short; from 80°, to 83°, 7° short, within
    // 11.25°; from 89°, the 1° left.
    assert_eq!(
        toward(0, 90),
        Turn {
            facing: facing(3),
            faces: false
        }
    );
    assert_eq!(
        toward(80, 90),
        Turn {
            facing: facing(83),
            faces: true
        }
    );
    assert_eq!(
        toward(89, 90),
        Turn {
            facing: facing(90),
            faces: true
        }
    );
    // The shorter side: from 10° toward 350°, back to 7°; half a turn, counter-clockwise.
    assert_eq!(
        toward(10, 350),
        Turn {
            facing: facing(7),
            faces: false
        }
    );
    assert_eq!(
        toward(0, 180),
        Turn {
            facing: facing(3),
            faces: false
        }
    );
    assert_eq!(
        toward(180, 0),
        Turn {
            facing: facing(183),
            faces: false
        }
    );
    // 11.25° short faces; a bit more does not.
    let short = Facing::of(Num::int(90) - Facing::WITHIN);
    let zero = TurnStep::new(Num::ZERO);
    assert!(short.toward(facing(90), zero).faces);
    let past = Facing::of(Num::int(90) - Facing::WITHIN - Num::EPSILON);
    assert!(!past.toward(facing(90), zero).faces);
    // With no step, at once.
    assert_eq!(
        facing(10).toward(facing(250), None),
        Turn {
            facing: facing(250),
            faces: true
        }
    );
}
