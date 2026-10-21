use super::*;
use crate::body_box::BodyBox;
use crate::radius::Radius;

fn circle(radius: i64, at: Vec3) -> Placed {
    Placed::new(at, Shape::Circle(Radius::new(Num::int(radius)).unwrap()))
}

fn at(x: Num, z: Num) -> Vec3 {
    Vec3::new(x, Num::ZERO, z)
}

#[test]
fn bodies_overlap_closer_than_their_edges_and_touching_is_apart() {
    let e = Num::EPSILON;
    let origin = at(Num::ZERO, Num::ZERO);
    // Circles of 1 and 2 m: their centers 3 m apart touch, a bit nearer overlap.
    let small = circle(1, origin);
    assert!(!small.overlaps(circle(2, at(Num::int(3), Num::ZERO))));
    assert!(small.overlaps(circle(2, at(Num::int(3) - e, Num::ZERO))));
    // A box of 2 × 2 m and a circle of 1 m: the circle's center 2 m off along x touches the
    // box's edge, a bit nearer overlaps, either way round; a point inside the open box overlaps.
    let boxed = Placed::new(
        origin,
        Shape::Box(BodyBox::new([Num::int(2); 2], Num::ZERO).unwrap()),
    );
    let touching = circle(1, at(Num::int(2), Num::ZERO));
    assert!(!boxed.overlaps(touching) && !touching.overlaps(boxed));
    let inside = circle(1, at(Num::int(2) - e, Num::ZERO));
    assert!(boxed.overlaps(inside) && inside.overlaps(boxed));
    assert!(boxed.overlaps(Placed::point(at(Num::HALF, Num::ZERO))));
    // Two such boxes side by side touch, and a bit nearer overlap.
    let other = |x: Num| Placed::new(at(x, Num::ZERO), boxed.shape);
    assert!(!boxed.overlaps(other(Num::int(2))));
    assert!(boxed.overlaps(other(Num::int(2) - e)));
}

#[test]
fn a_circle_overlaps_a_polygon_closer_than_its_radius_to_an_edge_or_inside_it() {
    // The square from (0, 0) to (4, 4), and circles of 5 m: one centered 5 m left of its left
    // edge touches it, a bit nearer overlaps; one 5 m from its corner along the 3-4-5 triangle
    // touches the corner, one nearer overlaps; one at its middle overlaps, its center inside.
    let square = Polygon::new([[0, 0], [4, 0], [4, 4], [0, 4]].map(|p| p.map(Num::int))).unwrap();
    let e = Num::EPSILON;
    let two = Num::int(2);
    for (x, z, overlaps) in [
        (Num::int(-5), two, false),
        (Num::int(-5) + e, two, true),
        (Num::int(-3), Num::int(-4), false),
        (Num::int(-3), Num::int(-3), true),
        (two, two, true),
    ] {
        let placed = circle(5, at(x, z));
        assert_eq!(placed.overlaps_polygon(&square), overlaps, "{x:?} {z:?}");
    }
    // A box of 2 × 2 m measures as `BodyBox::overlaps_polygon` does: centered 1 m left of the
    // square it touches it, a bit nearer it overlaps.
    let boxed = |x: Num| {
        let body = BodyBox::new([two; 2], Num::ZERO).unwrap();
        Placed::new(at(x, two), Shape::Box(body))
    };
    assert!(!boxed(-Num::ONE).overlaps_polygon(&square));
    assert!(boxed(-Num::ONE + e).overlaps_polygon(&square));
}
