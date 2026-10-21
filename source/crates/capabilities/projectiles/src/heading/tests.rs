use campfire_codec::Binary;

use super::*;

/// Whether `heading` lies within 4 bits, 2⁻²², of `want`'s decimals on each axis: within the
/// rounding of a unit and of the decimals' own.
fn near(heading: Heading, want: [&str; 3]) -> bool {
    let got = heading.get();
    [got.x, got.y, got.z]
        .into_iter()
        .zip(want.map(|text| text.parse::<Num>().unwrap()))
        .all(|(got, want)| (got - want).to_bits().abs() <= 4)
}

fn at(x: i64, y: i64, z: i64) -> Vec3 {
    Vec3::new(Num::int(x), Num::int(y), Num::int(z))
}

#[test]
fn a_flight_leaves_along_the_ground_raised_by_twice_its_climb() {
    let from = Vec3::ZERO;
    // Level: along x. Down 5 m: no lower. Up 5 m over 10 m: 2 · 5 / 10 = 1 up for 1 along, 45°.
    assert_eq!(Heading::toward(from, at(10, 0, 0)).get(), at(1, 0, 0));
    assert_eq!(Heading::toward(from, at(10, -5, 0)).get(), at(1, 0, 0));
    let half = "0.7071067812";
    assert!(near(Heading::toward(from, at(10, 5, 0)), [half, half, "0"]));
    // Up 1 m over 0.5 m, of at least 1 m: 2 up for 1 along, (1, 2, 0) / √5; straight up 5 m,
    // along x: (1, 10, 0) / √101.
    let short = Vec3::new(Num::HALF, Num::ONE, Num::ZERO);
    let five = ["0.4472135955", "0.8944271910", "0"];
    assert!(near(Heading::toward(from, short), five));
    let above = ["0.0995037190", "0.9950371902", "0"];
    assert!(near(Heading::toward(from, at(0, 5, 0)), above));
}

#[test]
fn a_heading_turns_toward_its_way_by_at_most_its_bound() {
    let x = Heading(at(1, 0, 0));
    let turn = |degrees: i64| Some(SinCos::of_degrees(Num::int(degrees)));
    // A quarter turn away, by 30° at most: (cos 30°, 0, sin 30°). Within 60°, the 45° way whole;
    // with no bound, a quarter turn whole.
    assert!(near(
        x.turned(at(0, 0, 1), turn(30)),
        ["0.8660254038", "0", "0.5"]
    ));
    let half = "0.7071067812";
    assert!(near(x.turned(at(1, 0, 1), turn(60)), [half, "0", half]));
    assert_eq!(x.turned(at(0, 0, 1), None).get(), at(0, 0, 1));
    // Straight back, by a quarter turn: about the vertical, to the ground's perpendicular, (0, 0,
    // 1). A way of no length keeps it.
    assert!(near(x.turned(at(-1, 0, 0), turn(90)), ["0", "0", "1"]));
    assert_eq!(x.turned(Vec3::ZERO, turn(30)), x);
    // A snapshot's unit decodes as it is; a way of length 2 does not.
    let unit = x.turned(at(0, 0, 1), turn(30));
    assert_eq!(
        Binary::decode::<Heading>(&Binary::encode(&unit)).unwrap(),
        unit
    );
    assert!(Binary::decode::<Heading>(&Binary::encode(&at(2, 0, 0))).is_err());
}
