use super::*;
use crate::body_bound::MAX_BOUND;
use crate::radius::Radius;

#[test]
fn a_reach_runs_from_edge_to_edge() {
    let num = |value| Num::from_int(value).unwrap();
    let (from, to) = (Vec3::from((0, 0, 0)), Vec3::from((10, 0, 0)));
    // 10 m between centers: a range of 7 from a body of 1 to one of 2 touches, so reaches, 7
    // less a bit falls short, and 8 passes; a point reaches as a body of 0.
    let cases = [
        (num(1), num(7), num(2), Ordering::Equal),
        (num(1), num(7) - Num::EPSILON, num(2), Ordering::Greater),
        (num(1), num(8), num(2), Ordering::Less),
        (Num::ZERO, num(10), Num::ZERO, Ordering::Equal),
        (Num::ZERO, num(8), num(2), Ordering::Equal),
        (Num::ZERO, num(8) - Num::EPSILON, num(2), Ordering::Greater),
        // A reach past what a number holds reaches all.
        (MAX_BOUND, Num::MAX, Num::ZERO, Ordering::Less),
    ];
    let circle = |radius| Shape::Circle(Radius::new(radius).unwrap());
    for (from_radius, range, to_radius, order) in cases {
        let (a, b) = (
            Placed::new(from, circle(from_radius)),
            Placed::new(to, circle(to_radius)),
        );
        let case = format!("{from_radius:?} + {range:?} + {to_radius:?}");
        assert_eq!(Metric::Planar.order(a, range, b), order, "{case}");
        assert_eq!(
            Metric::Planar.reaches(a, range, b),
            order != Ordering::Greater,
            "{case}"
        );
    }
    // 4 m up and 3 m along: 3 apart on a planar map, 5 on a spatial one.
    let above = Vec3::from((3, 4, 0));
    let point = Shape::POINT;
    assert!(Metric::Planar.reaches(Placed::new(from, point), num(3), Placed::new(above, point)));
    assert!(!Metric::Spatial.reaches(Placed::new(from, point), num(4), Placed::new(above, point)));
    assert!(Metric::Spatial.reaches(
        Placed::new(from, circle(num(1))),
        num(3),
        Placed::new(above, circle(num(1)))
    ));
    // A box of 4 × 2 m at 10 m along x, its near edge at 8 m: a point reaches it at 8, a
    // body of 1 m at 7, either way round, and a box of 2 × 2 m at the start, its edge at 1 m,
    // at 7, not 7 less a bit. A reach past what a number holds reaches all.
    let far = Shape::Box(BodyBox::new([num(4), num(2)], Num::ZERO).unwrap());
    let near = Shape::Box(BodyBox::new([num(2), num(2)], Num::ZERO).unwrap());
    let shapes = [
        (point, num(8), far, true),
        (point, num(8) - Num::EPSILON, far, false),
        (circle(num(1)), num(7), far, true),
        (circle(num(1)), num(7) - Num::EPSILON, far, false),
        (near, num(7), far, true),
        (near, num(7) - Num::EPSILON, far, false),
        (circle(num(1)), Num::MAX, far, true),
    ];
    for (shape, range, other, reaches) in shapes {
        let there = Metric::Planar.reaches(Placed::new(from, shape), range, Placed::new(to, other));
        let back = Metric::Planar.reaches(Placed::new(to, other), range, Placed::new(from, shape));
        assert_eq!((there, back), (reaches, reaches), "{shape:?} {range:?}");
    }
}

#[test]
fn a_direction_and_a_step_keep_to_the_ground_on_a_planar_map() {
    let num = |value| Num::from_int(value).unwrap();
    let vector = |x, y, z| Vec3::new(num(x), num(y), num(z));
    // 3 along and 4 up: on the ground, straight along x; straight up has no ground direction.
    assert_eq!(
        Metric::Planar.direction(vector(3, 4, 0)),
        Some(vector(1, 0, 0))
    );
    assert_eq!(Metric::Planar.direction(vector(0, 5, 0)), None);
    assert_eq!(
        Metric::Spatial.direction(vector(0, 5, 0)),
        Some(vector(0, 1, 0))
    );
    // A step of 1 toward (3, 4, 0): along the ground at the height it starts at on a planar
    // map, along the line in space on a spatial one. A step past the end ends there, which on
    // a planar map is the point below or above at the start's height.
    let (from, to) = (Vec3::from((0, 0, 0)), Vec3::from((3, 4, 0)));
    assert_eq!(
        Metric::Planar.step_toward(from, to, num(1)),
        Vec3::from((1, 0, 0))
    );
    let spatial = from.step_toward(to, num(1));
    assert_eq!(Metric::Spatial.step_toward(from, to, num(1)), spatial);
    assert_eq!(
        Metric::Planar.step_toward(from, to, num(5)),
        Vec3::from((3, 0, 0))
    );
    assert_eq!(Metric::Spatial.step_toward(from, to, num(5)), to);
}

#[test]
fn a_path_meets_a_point_within_reach_of_its_nearest_point() {
    let num = |value| Num::from_int(value).unwrap();
    let (from, to) = (Vec3::from((0, 0, 0)), Vec3::from((10, 0, 0)));
    let point = Shape::POINT;
    // Beside the path 2 m off, at 4 m along: met within 2, at 4 m along, and not within
    // 2 less a bit. Behind the start and past the end, the ends count.
    let cases = [
        (Vec3::from((4, 0, 2)), num(2), Some(num(4))),
        (Vec3::from((4, 0, 2)), num(2) - Num::EPSILON, None),
        (Vec3::from((-1, 0, 0)), num(1), Some(Num::ZERO)),
        (Vec3::from((-2, 0, 0)), num(1), None),
        (Vec3::from((12, 0, 0)), num(2), Some(num(10))),
        (Vec3::from((12, 0, 0)), num(1), None),
    ];
    for (at, reach, expected) in cases {
        let met = Metric::Planar.meets(from, to, Placed::new(at, point), reach);
        assert_eq!(met, expected, "{at:?} within {reach:?}");
    }
    // A circle of 1 m is met within a reach 1 m shorter.
    let circle = Shape::Circle(Radius::new(num(1)).unwrap());
    assert_eq!(
        Metric::Planar.meets(from, to, Placed::new(Vec3::from((4, 0, 2)), circle), num(1)),
        cases[0].2
    );
    // 3 m up: a planar map ignores the height, a spatial one counts it, √(4 + 9) > 3.
    let above = Vec3::from((4, 3, 2));
    // Its place on the path is the point's along it, 4 m, either way.
    let along = Some(num(4));
    assert_eq!(
        Metric::Planar.meets(from, to, Placed::new(above, point), num(2)),
        along
    );
    assert_eq!(
        Metric::Spatial.meets(from, to, Placed::new(above, point), num(3)),
        None
    );
    assert_eq!(
        Metric::Spatial.meets(from, to, Placed::new(above, point), num(4)),
        along
    );
    // A path of no length meets what is within reach of its point.
    let still = Metric::Planar.meets(
        from,
        from,
        Placed::new(Vec3::from((0, 0, 1)), point),
        num(1),
    );
    assert_eq!(still, Some(Num::ZERO));
    // A box of 4 × 2 m at 5 m along, through which the path runs: met where it enters, at
    // 3 m, whatever the reach.
    let body = Shape::Box(BodyBox::new([num(4), num(2)], Num::ZERO).unwrap());
    let entered = Metric::Planar.meets(
        from,
        to,
        Placed::new(Vec3::from((5, 0, 0)), body),
        Num::ZERO,
    );
    assert_eq!(entered, Some(num(3)));
    // A reach and a radius whose sum passes every number reach every distance, as
    // `reaches` counts them: met at the path's nearest point, 4 m along, and come within.
    let huge = Shape::Circle(Radius::new(MAX_BOUND).unwrap());
    let far = Vec3::from((4, 0, 900));
    assert_eq!(
        Metric::Planar.meets(from, to, Placed::new(far, huge), Num::MAX),
        Some(num(4))
    );
    assert!(Placed::new(far, huge).comes_closer(from, to, Num::MAX));
    assert!(Metric::Planar.reaches(Placed::new(from, huge), Num::MAX, Placed::new(far, point)));
}
