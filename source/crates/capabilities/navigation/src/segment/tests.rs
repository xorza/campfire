use campfire_math::Num;

use super::*;

#[test]
fn a_segment_comes_closer_a_reach_of_a_point_exactly() {
    let e = Num::EPSILON;
    // Along x from 0 to 10: (5, 1) is 1 m off it; (−3, 4) and (13, 4) are 5 m from an end,
    // before its start and past its end.
    let straight = Segment::new(Vec3::from((0, 0, 0)), Vec3::from((10, 0, 0)));
    for (point, distance) in [
        (Vec3::from((5, 0, 1)), 1),
        (Vec3::from((-3, 0, 4)), 5),
        (Vec3::from((13, 0, 4)), 5),
    ] {
        assert!(
            !straight.comes_closer(point, Num::int(distance)),
            "{point:?} touches"
        );
        assert!(
            straight.comes_closer(point, Num::int(distance) + e),
            "{point:?} within"
        );
    }
    // From (0, 0) to (8, 6), 10 m: (1, 7) projects halfway, onto (4, 3), √(3² + 4²) = 5 m
    // away.
    let slant = Segment::new(Vec3::from((0, 0, 0)), Vec3::from((8, 0, 6)));
    assert!(!slant.comes_closer(Vec3::from((1, 0, 7)), Num::int(5)));
    assert!(slant.comes_closer(Vec3::from((1, 0, 7)), Num::int(5) + e));
    // A segment from a point to itself is that point: (5, 6) is 5 m from (2, 2).
    let only = Segment::new(Vec3::from((2, 0, 2)), Vec3::from((2, 0, 2)));
    assert!(!only.comes_closer(Vec3::from((5, 0, 6)), Num::int(5)));
    assert!(only.comes_closer(Vec3::from((5, 0, 6)), Num::int(5) + e));
    // Across the whole world, 2²⁰ m, whose products pass 128 bits: (0, 1) is 1 m off.
    let half: i32 = 1 << 19;
    let end = |x: i32| Vec3::from((x, 0, 0));
    let wide = Segment::new(end(-half), end(half));
    assert!(!wide.comes_closer(Vec3::from((0, 0, 1)), Num::int(1)));
    assert!(wide.comes_closer(Vec3::from((0, 0, 1)), Num::int(1) + e));
}
