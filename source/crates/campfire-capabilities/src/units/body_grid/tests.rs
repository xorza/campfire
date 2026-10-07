use campfire_math::Vec3;
use campfire_sim::IdAllocator;

use super::*;

/// `tenths` of a meter.
fn m(tenths: i64) -> Num {
    Num::from_int(tenths).unwrap() / 10
}

/// A body of `radius` tenths at `[x, z]` tenths, with the next id.
fn placed(ids: &mut IdAllocator, [x, z]: [i64; 2], radius: i64) -> Placed<()> {
    Placed {
        id: ids.allocate(),
        key: (),
        at: Position::new(Vec3::new(m(x), Num::ZERO, m(z))).unwrap(),
        radius: m(radius),
    }
}

/// The ids `grid` visits for the box from `low` to `high`, in tenths, sorted, each once.
fn visited(grid: &BodyGrid<()>, low: [i64; 2], high: [i64; 2]) -> Vec<StableId> {
    let mut ids = Vec::new();
    grid.visit(low.map(m), high.map(m), |body| ids.push(body.id));
    let count = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), count, "each body once");
    ids
}

#[test]
fn a_box_visits_the_bodies_of_the_cells_it_covers_grown_by_the_widest() {
    let mut ids = IdAllocator::default();
    let mut grid = BodyGrid::default();
    assert_eq!(visited(&grid, [0, 0], [10, 10]), []);
    // Bodies of radius 0.5 m: a cell is 2 × 0.5 = 1 m.
    let at = [[0, 0], [20, 0], [50, 0], [-30, 0], [0, 40]];
    let bodies = at.map(|at| placed(&mut ids, at, 5));
    grid.rebuild(bodies);
    let [a, b, _, d, e] = bodies.map(|body| body.id);
    // x 1 to 3 m, grown to 0.5 to 3.5, columns 0 to 3; z -1 to 1 m, grown to -1.5 to 1.5,
    // rows -2 to 1, 4 rows of 5 bodies: the cells of a, at column 0, and b, at 2.
    assert_eq!(visited(&grid, [10, -10], [30, 10]), [a, b]);
    // z -10 to 10 m covers 22 rows, more than the 5 bodies, so one pass over them all finds
    // the same cells: e, at column 0 row 4, too.
    assert_eq!(visited(&grid, [10, -100], [30, 100]), [a, b, e]);
    // Cells below 0 round down: -3.2 to -2.8 m, grown to -3.7 to -2.3, columns -4 to -3.
    assert_eq!(visited(&grid, [-32, -2], [-28, 2]), [d]);
    // Points alone take cells of a meter: -0.4 to 0.4 m is columns and rows -1 to 0.
    grid.rebuild(at.map(|at| placed(&mut ids, at, 0)));
    assert_eq!(visited(&grid, [-4, -4], [4, 4]).len(), 1);
}

#[test]
fn a_box_meets_every_body_whose_square_overlaps_it() {
    let mut ids = IdAllocator::default();
    // A lattice 0.7 m apart from -5.6 to 5.6 m, radii 0 to 1.2 m: cells of 2.4 m, rows -3 to 2.
    let mut lattice = Vec::new();
    for row in -8_i64..=8 {
        for column in -8..=8 {
            let radius = (row * 3 + column * 5).rem_euclid(13);
            lattice.push(placed(&mut ids, [column * 7, row * 7], radius));
        }
    }
    // Two points 50 km away either way spread the rows over 41 668, more than 4 × 291 + 64 =
    // 1228, so the grid keeps no row starts and searches all its bodies for each row.
    let far = [-500_000, 500_000].map(|z| placed(&mut ids, [0, z], 0));
    for outliers in [false, true] {
        let mut bodies = lattice.clone();
        if outliers {
            bodies.extend(far);
        }
        let mut grid = BodyGrid::default();
        grid.rebuild(bodies.iter().copied());
        let kept = grid.rows.row((), 0);
        assert_eq!(matches!(kept, Some(RowEntries::Layer(_))), outliers);
        for (low, high) in [
            ([0, 0], [0, 0]),
            ([-13, 4], [9, 21]),
            ([-60, -60], [60, 60]),
            ([-200, -3], [200, 3]),
            ([33, -47], [34, -46]),
            // Past the last row, in part and wholly.
            ([0, 50], [10, 300]),
            ([0, 500], [10, 600]),
        ] {
            let got = visited(&grid, low, high);
            let overlaps = |body: &&Placed<()>| {
                let at = body.at.get();
                let near = |axis: Num, low: i64, high: i64| {
                    m(low) <= axis + body.radius && axis - body.radius <= m(high)
                };
                near(at.x, low[0], high[0]) && near(at.z, low[1], high[1])
            };
            for body in bodies.iter().filter(overlaps) {
                assert!(
                    got.binary_search(&body.id).is_ok(),
                    "{low:?} {high:?} {body:?}"
                );
            }
            // Exactly the bodies of the cells the box covers grown by the widest body, 1.2 m,
            // whether row starts or searches find them.
            let cell = |tenths: Num| tenths.to_bits().div_euclid(grid.cell);
            let inside = |body: &&Placed<()>| {
                let at = body.at.get();
                let within = |axis: Num, low: i64, high: i64| {
                    (cell(m(low) - m(12))..=cell(m(high) + m(12))).contains(&cell(axis))
                };
                within(at.x, low[0], high[0]) && within(at.z, low[1], high[1])
            };
            let mut expected: Vec<_> = bodies.iter().filter(inside).map(|body| body.id).collect();
            expected.sort_unstable();
            assert_eq!(got, expected, "{low:?} {high:?}");
        }
    }
}
