use campfire_math::{Num, Vec3};

use super::*;

#[test]
fn a_segment_comes_within_a_reach_of_a_point_exactly() {
    let e = Num::EPSILON;
    // Along x from 0 to 10: (5, 1) is 1 m off it; (−3, 4) and (13, 4) are 5 m from an end,
    // before its start and past its end.
    let straight = Segment::new(Position::from((0, 0, 0)), Position::from((10, 0, 0)));
    for (point, distance) in [
        (Position::from((5, 0, 1)), 1),
        (Position::from((-3, 0, 4)), 5),
        (Position::from((13, 0, 4)), 5),
    ] {
        assert!(
            !straight.comes_within(point, Num::int(distance)),
            "{point:?} touches"
        );
        assert!(
            straight.comes_within(point, Num::int(distance) + e),
            "{point:?} within"
        );
    }
    // From (0, 0) to (8, 6), 10 m: (1, 7) projects halfway, onto (4, 3), √(3² + 4²) = 5 m
    // away.
    let slant = Segment::new(Position::from((0, 0, 0)), Position::from((8, 0, 6)));
    assert!(!slant.comes_within(Position::from((1, 0, 7)), Num::int(5)));
    assert!(slant.comes_within(Position::from((1, 0, 7)), Num::int(5) + e));
    // A segment from a point to itself is that point: (5, 6) is 5 m from (2, 2).
    let only = Segment::new(Position::from((2, 0, 2)), Position::from((2, 0, 2)));
    assert!(!only.comes_within(Position::from((5, 0, 6)), Num::int(5)));
    assert!(only.comes_within(Position::from((5, 0, 6)), Num::int(5) + e));
    // Across the whole world, 2²⁰ m, whose products pass 128 bits: (0, 1) is 1 m off.
    let half: i32 = 1 << 19;
    let end = |x: i32| Position::try_from(Vec3::from((x, 0, 0))).unwrap();
    let wide = Segment::new(end(-half), end(half));
    assert!(!wide.comes_within(Position::from((0, 0, 1)), Num::int(1)));
    assert!(wide.comes_within(Position::from((0, 0, 1)), Num::int(1) + e));
}
