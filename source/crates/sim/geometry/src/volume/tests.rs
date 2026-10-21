use super::*;
use crate::body_box::BodyBox;
use crate::radius::Radius;

#[test]
fn a_volume_reaches_from_its_sides_top_and_bottom_in_space() {
    let num = Num::int;
    // A cylinder of radius 1 and height 2 at the origin: 2 m beside it at mid height, 3 m
    // above its top and 3 m below its bottom, and 3 m out and 4 m up from its top edge, 5 m.
    let circle = Shape::Circle(Radius::new(Num::ONE).unwrap());
    let cylinder = Volume::new(Placed::new(Vec3::ZERO, circle), num(2));
    let beside = Vec3::from((3, 1, 0));
    let above = Vec3::from((0, 5, 0));
    let below = Vec3::from((0, -3, 0));
    let corner = Vec3::from((4, 6, 0));
    for (point, distance) in [(beside, 2), (above, 3), (below, 3), (corner, 5)] {
        let at = |reach: Num| cylinder.nearest(point, reach);
        assert_eq!(at(num(distance)), Ordering::Equal, "{point:?}");
        assert_eq!(
            at(num(distance) - Num::EPSILON),
            Ordering::Greater,
            "{point:?}"
        );
        assert_eq!(
            at(num(distance) + Num::EPSILON),
            Ordering::Less,
            "{point:?}"
        );
        // A reach of exactly the distance reaches it, and one a bit short does not.
        assert!(cylinder.reaches(point, num(distance)), "{point:?}");
        assert!(
            !cylinder.reaches(point, num(distance) - Num::EPSILON),
            "{point:?}"
        );
    }
    // Its height counts: 4 m tall, the point 5 m up lies 1 m above it.
    let taller = Volume::new(cylinder.base, num(4));
    assert_eq!(taller.nearest(above, Num::ONE), Ordering::Equal);
    // Inside it is closer than every reach; on its top, at 0, as on a planar edge.
    assert_eq!(
        cylinder.nearest(Vec3::from((0, 1, 0)), Num::ZERO),
        Ordering::Less
    );
    let top = Vec3::from((0, 2, 0));
    assert_eq!(cylinder.nearest(top, Num::ZERO), Ordering::Equal);
    assert_eq!(cylinder.nearest(top, Num::EPSILON), Ordering::Less);
    // A flat volume is its shape at its base's height.
    let flat = Volume::new(cylinder.base, Num::ZERO);
    assert_eq!(flat.nearest(Vec3::from((0, 3, 0)), num(3)), Ordering::Equal);

    // A box 4 × 2 m, height 3, at (10, 1, 0): its side 2 m from its base's x. From (15, 8, 0),
    // 3 m past that side and 4 m above its top at 4 m, 5 m; from (13, 2, 0), 1 m beside it.
    let body = BodyBox::new([num(4), num(2)], Num::ZERO).unwrap();
    let boxed = Volume::new(
        Placed::new(Vec3::from((10, 1, 0)), Shape::Box(body)),
        num(3),
    );
    assert_eq!(
        boxed.nearest(Vec3::from((15, 8, 0)), num(5)),
        Ordering::Equal
    );
    assert_eq!(
        boxed.nearest(Vec3::from((15, 8, 0)), num(5) - Num::EPSILON),
        Ordering::Greater
    );
    assert_eq!(
        boxed.nearest(Vec3::from((13, 2, 0)), Num::ONE),
        Ordering::Equal
    );
    assert_eq!(
        boxed.nearest(Vec3::from((11, 2, 0)), Num::ZERO),
        Ordering::Less
    );
    // Its center is halfway up: 1 + 3 ÷ 2.
    assert_eq!(boxed.center(), Vec3::new(num(10), Num::HALF * 5, Num::ZERO));
}
