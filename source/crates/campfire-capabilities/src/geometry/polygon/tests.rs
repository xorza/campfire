use super::*;
use crate::geometry::bounds::Bounds;

fn points(points: &[[i64; 2]]) -> Vec<[Num; 2]> {
    points.iter().map(|point| point.map(Num::int)).collect()
}

fn polygon(of: &[[i64; 2]]) -> Polygon {
    Polygon::new(points(of)).unwrap()
}

/// A point `[x, z]` in quarters of a meter, in halves of a bit.
fn quarters([x, z]: [i64; 2]) -> [i128; 2] {
    [x, z].map(|value| 2 * i128::from(Num::QUARTER.to_bits()) * i128::from(value))
}

#[test]
fn a_polygon_needs_three_points_and_edges_that_meet_only_where_they_follow() {
    let refused = [
        (vec![[0, 0], [1, 0]], PolygonError::TooFewPoints),
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
}

#[test]
fn a_polygon_holds_the_cells_whose_centers_lie_inside_it_or_on_its_edge() {
    let bounds = Bounds::new([Num::ZERO; 2], [Num::int(4); 2]).unwrap();
    let cells = |polygon: &Polygon, cell: Num| {
        let grid = Grid::new(cell, bounds).unwrap();
        let mut held = Vec::new();
        polygon.cells(&grid, |cell| held.push(cell));
        held
    };
    // The triangle (0, 0), (4, 0), (0, 4) holds x + z ≤ 4. Its 1 m cells' centers are at
    // (i + ½, j + ½), held when i + j ≤ 3, those with i + j = 3 on the long edge: numbered 4j + i.
    let triangle = polygon(&[[0, 0], [4, 0], [0, 4]]);
    assert_eq!(cells(&triangle, Num::ONE), [0, 1, 2, 3, 4, 5, 6, 8, 9, 12]);
    // Half-meter cells, centers at ((i + ½) / 2, (j + ½) / 2), held when i + j ≤ 7: 8 + 7 + … + 1.
    let half: Vec<usize> = (0..64).filter(|cell| cell % 8 + cell / 8 <= 7).collect();
    assert_eq!(half.len(), 36);
    assert_eq!(cells(&triangle, Num::HALF), half);
    // The box from (1, 1) to (3, 3): 1 m centers at 1.5 and 2.5 are inside, 0.5 and 3.5 out; half
    // meter centers from 1.25 to 2.75, columns and rows 2 to 5; 2 m centers at 1 and 3, on its
    // corners, all four.
    let square = polygon(&[[1, 1], [3, 1], [3, 3], [1, 3]]);
    assert_eq!(cells(&square, Num::ONE), [5, 6, 9, 10]);
    let middle: Vec<usize> = (2..=5)
        .flat_map(|row| (2..=5).map(move |column| row * 8 + column))
        .collect();
    assert_eq!(cells(&square, Num::HALF), middle);
    assert_eq!(cells(&square, Num::int(2)), [0, 1, 2, 3]);
}
