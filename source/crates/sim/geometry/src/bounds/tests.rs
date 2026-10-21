use super::*;

#[test]
fn bounds_hold_their_closed_rectangle_and_clamp_to_it() {
    let bounds = Bounds::new([Num::int(-2), Num::int(-1)], [Num::int(4), Num::int(3)]).unwrap();
    let e = Num::EPSILON;
    // Corners and edges are inside; a bit past any edge is not. Height counts only past the
    // world's bound: 50 m up is inside, a bit past 2²⁰ m, up or down, is not.
    for (x, z, inside) in [
        (Num::int(-2), Num::int(-1), true),
        (Num::int(4), Num::int(3), true),
        (Num::int(4), Num::int(0), true),
        (Num::int(4) + e, Num::int(0), false),
        (Num::int(-2) - e, Num::int(0), false),
        (Num::int(0), Num::int(3) + e, false),
        (Num::int(0), Num::int(-1) - e, false),
    ] {
        assert_eq!(
            bounds.contains(Vec3::from((x, 50, z))),
            inside,
            "{x:?} {z:?}"
        );
    }
    // (9, 9) clamps to the corner (4, 3), (−5, 1) to the left edge at z = 1, and a point
    // inside stays, each at its own height.
    assert_eq!(bounds.clamp(Vec3::from((9, 7, 9))), Vec3::from((4, 7, 3)));
    assert_eq!(bounds.clamp(Vec3::from((-5, 5, 1))), Vec3::from((-2, 5, 1)));
    assert_eq!(bounds.clamp(Vec3::from((1, 0, 2))), Vec3::from((1, 0, 2)));
    assert_eq!(
        bounds.clamp_ground([Num::int(-9), Num::int(-9)]),
        [Num::int(-2), Num::int(-1)]
    );
    assert_eq!(
        bounds.ground_point([Num::int(9), Num::int(-9)], Vec3::from((1, 6, 2))),
        Vec3::from((4, 6, -1))
    );

    let past = Vec3::BOUND + e;
    for (min, max) in [
        ([Num::int(0), Num::int(0)], [Num::int(0), Num::int(1)]),
        ([Num::int(1), Num::int(0)], [Num::int(0), Num::int(1)]),
        ([Num::int(0), Num::int(0)], [past, Num::int(1)]),
        ([Num::int(0), -past], [Num::int(1), Num::int(1)]),
    ] {
        assert_eq!(Bounds::new(min, max), None, "{min:?} {max:?}");
    }
    assert!(Bounds::WORLD.contains(Vec3::from((-Vec3::BOUND, 0, Vec3::BOUND))));
    // A box of 2 × 1 m, half extents of 1 and 0.5, lies within them from x = −1 to 3 and
    // z = −0.5 to 2.5, its edges on theirs; a bit past either end is out.
    let boxed = BodyBox::new([Num::int(2), Num::ONE], Num::ZERO).unwrap();
    let half = Num::HALF;
    for (x, z, held) in [
        (Num::int(-1), -half, true),
        (Num::int(3), Num::int(3) - half, true),
        (Num::int(3) + e, Num::ZERO, false),
        (Num::int(-1) - e, Num::ZERO, false),
        (Num::ZERO, Num::int(3) - half + e, false),
        (Num::ZERO, -half - e, false),
    ] {
        let position = Vec3::new(x, Num::ZERO, z);
        assert_eq!(bounds.holds_box(position, &boxed), held, "{x:?} {z:?}");
    }
    for height in [Vec3::BOUND, -Vec3::BOUND] {
        assert!(bounds.contains(Vec3::new(Num::ZERO, height, Num::ZERO)));
    }
    for height in [past, -past] {
        assert!(!bounds.contains(Vec3::new(Num::ZERO, height, Num::ZERO)));
    }

    // From (0, 0, 1): 4 to the right edge along +x, 2 to the bottom along −z, and from the
    // right edge on along +x, 0. From a point past the right edge, 0, also back along −x and
    // straight up, which no ground axis bounds. Straight up, the path leaves at the world's
    // height.
    let x = Vec3::new(Num::ONE, Num::ZERO, Num::ZERO);
    let z = Vec3::new(Num::ZERO, Num::ZERO, -Num::ONE);
    let up = Vec3::new(Num::ZERO, Num::ONE, Num::ZERO);
    let origin = Vec3::from((0, 0, 1));
    assert_eq!(bounds.exit(origin, x), Num::int(4));
    assert_eq!(bounds.exit(origin, z), Num::int(2));
    assert_eq!(bounds.exit(Vec3::from((4, 0, 1)), x), Num::ZERO);
    let past = Vec3::from((5, 0, 1));
    for direction in [x, -x, up] {
        assert_eq!(bounds.exit(past, direction), Num::ZERO, "{direction:?}");
    }
    assert_eq!(bounds.exit(origin, up), Vec3::BOUND);
    // Along (0.6, 0, 0.8), each rounded once, the right edge lies 3 / 0.600 000 02 =
    // 4.999 999 8 m on, 3 · 2⁴⁸ / 10 066 330 = 83 886 076.67 bits, rounded down, as the
    // nearest, 83 886 077, would carry the path a hair past it: the distance times the
    // step's x stays within the 3 m.
    let slant = Vec3::new(
        Num::from_bits(10_066_330),
        Num::ZERO,
        Num::from_bits(13_421_773),
    );
    let from = Vec3::from((1, 0, -1));
    let exit = bounds.exit(from, slant);
    assert_eq!(exit, Num::from_bits(83_886_076));
    let reached = i128::from(exit.to_bits()) * i128::from(slant.x.to_bits());
    assert!(reached <= 3 << 48);
    assert!(reached + i128::from(slant.x.to_bits()) > 3 << 48);
}
