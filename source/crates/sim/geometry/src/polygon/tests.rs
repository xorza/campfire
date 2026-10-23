use super::*;

fn points(points: &[[i64; 2]]) -> Vec<[Num; 2]> {
    points.iter().map(|point| point.map(Num::int)).collect()
}

fn polygon(of: &[[i64; 2]]) -> Polygon {
    Polygon::new(points(of)).unwrap()
}

/// A point `[x, z]` in quarters of a meter, in halves of a bit.
fn quarters([x, z]: [i64; 2]) -> Flat {
    Flat::new(x.into(), z.into()) * (2 * Num::QUARTER.to_bits())
}

#[test]
fn a_polygon_needs_three_points_and_edges_that_meet_only_where_they_follow() {
    let refused = [
        (vec![[0, 0], [1, 0]], PolygonError::TooFewPoints),
        // One point past the most a polygon holds, refused before its edges are tested; at the
        // most, its edges are, and its zero-length first one meets the next.
        (
            vec![[0, 0]; Polygon::MAX_POINTS + 1],
            PolygonError::TooManyPoints,
        ),
        (
            vec![[0, 0]; Polygon::MAX_POINTS],
            PolygonError::EdgesMeet {
                first: 0,
                second: 1,
            },
        ),
        // A bow tie: edges 0, (0, 0) to (2, 2), and 2, (2, 0) to (0, 2), cross at (1, 1).
        (
            vec![[0, 0], [2, 2], [2, 0], [0, 2]],
            PolygonError::EdgesMeet {
                first: 0,
                second: 2,
            },
        ),
        // A point twice: edge 1 has no length.
        (
            vec![[0, 0], [2, 0], [2, 0], [0, 2]],
            PolygonError::EdgesMeet {
                first: 0,
                second: 1,
            },
        ),
        // Three points on a line: edge 2, (4, 0) back to (0, 0), runs back over edge 0, the pair
        // tested first.
        (
            vec![[0, 0], [2, 0], [4, 0]],
            PolygonError::EdgesMeet {
                first: 0,
                second: 2,
            },
        ),
        // A point on an edge that does not follow it: edge 2 ends on (1, 0), which lies on edge 0.
        (
            vec![[0, 0], [4, 0], [4, 4], [1, 0]],
            PolygonError::EdgesMeet {
                first: 0,
                second: 2,
            },
        ),
    ];
    for (at, error) in refused {
        assert_eq!(Polygon::new(points(&at)), Err(error), "{at:?}");
    }
    // A point past the world's bound, 2²⁰ m, by a bit, on either axis and either side, or at a
    // number's end; a triangle to the bound itself is one.
    let bound = Vec3::BOUND;
    let bit = Num::from_bits(1);
    for past in [
        [bound + bit, Num::ZERO],
        [Num::ZERO, -bound - bit],
        [Num::MIN, Num::MAX],
    ] {
        let at = [[Num::ZERO; 2], [Num::ONE, Num::ZERO], past];
        assert_eq!(
            Polygon::new(at),
            Err(PolygonError::PastBound { point: 2 }),
            "{past:?}"
        );
    }
    let corner = Polygon::new([[-bound, -bound], [bound, -bound], [bound, bound]]);
    assert!(corner.unwrap().holds_point([bound, Num::ZERO]));
    // A triangle, a box, and a concave arrow, each simple; a point on a straight run between two
    // edges is no fault.
    for at in [
        vec![[0, 0], [4, 0], [0, 4]],
        vec![[0, 0], [2, 0], [4, 0], [4, 4], [0, 4]],
        vec![[0, 0], [4, 2], [0, 4], [1, 2]],
    ] {
        assert!(Polygon::new(points(&at)).is_ok(), "{at:?}");
    }
}

#[test]
fn a_polygon_holds_the_points_inside_it_and_on_its_edge_exactly() {
    // The arrow (0, 0), (4, 2), (0, 4), (1, 2), in quarters of a meter: its notch at x = 1.
    let arrow = polygon(&[[0, 0], [4, 2], [0, 4], [1, 2]]);
    let held = [
        // Inside, at the middle of the tip.
        [12, 8],
        // On the vertices, and on the edge from (0, 0) to (4, 2) at (2, 1).
        [0, 0],
        [16, 8],
        [4, 8],
        [8, 4],
        // On the notch's edge from (1, 2) to (0, 0), at (0.5, 1).
        [2, 4],
    ];
    let out = [
        // In the notch, a quarter left of its edge at z = 1, and left of its point.
        [1, 4],
        [3, 8],
        // A bit beyond the tip, and right of the edge at (2, 1).
        [17, 8],
        [8, 3],
    ];
    for at in held {
        assert!(arrow.holds(quarters(at)), "{at:?}");
    }
    for at in out {
        assert!(!arrow.holds(quarters(at)), "{at:?}");
    }
    // Inside alone, off the edge: the tip's middle, not the vertices or the edges' points, and
    // nothing outside.
    let quartered = |at: [i64; 2]| at.map(|at| Num::QUARTER * Num::int(at));
    let inside: Vec<bool> = held
        .iter()
        .map(|&at| arrow.holds_inside(quartered(at)))
        .collect();
    assert_eq!(inside, [true, false, false, false, false, false]);
    assert!(out.iter().all(|&at| !arrow.holds_inside(quartered(at))));
}
