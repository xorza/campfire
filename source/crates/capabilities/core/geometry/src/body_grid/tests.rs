use super::*;
use crate::radius::Radius;
use crate::shape::Shape;

/// `tenths` of a meter.
fn m(tenths: i64) -> Num {
    Num::from_int(tenths).unwrap() / 10
}

/// A body of `radius` tenths at `[x, z]` tenths, with the next id.
fn placed(ids: &mut u64, [x, z]: [i64; 2], radius: i64) -> Indexed<(), u64> {
    *ids += 1;
    Indexed {
        id: *ids,
        key: (),
        body: Placed::new(
            Vec3::new(m(x), Num::ZERO, m(z)),
            Shape::Circle(Radius::new(m(radius)).unwrap()),
        ),
    }
}

/// The ids `grid` visits for the box from `low` to `high`, in tenths, sorted, each once.
fn visited(grid: &BodyGrid<(), u64>, low: [i64; 2], high: [i64; 2]) -> Vec<u64> {
    let mut ids = Vec::new();
    grid.visit(low.map(m), high.map(m), |body| {
        ids.push(body.id);
        ControlFlow::Continue(())
    });
    let count = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), count, "each body once");
    ids
}

#[test]
fn a_box_visits_the_bodies_of_the_cells_it_covers_grown_by_the_widest() {
    let mut ids = 0;
    let mut grid = BodyGrid::default();
    assert!(visited(&grid, [0, 0], [10, 10]).is_empty());
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
    // A visit that breaks reads no body after: the box of a and b, by rows, read to its first
    // body, its second and past both; the box of three, by one pass, to its first, second and
    // past all three.
    let reads = |low: [i64; 2], high: [i64; 2], stop: usize| {
        let mut read = 0;
        grid.visit(low.map(m), high.map(m), |_| {
            read += 1;
            if read == stop {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        });
        read
    };
    assert_eq!(
        [1, 2, 3].map(|stop| reads([10, -10], [30, 10], stop)),
        [1, 2, 2]
    );
    assert_eq!(
        [1, 2, 4].map(|stop| reads([10, -100], [30, 100], stop)),
        [1, 2, 3]
    );
    // Cells below 0 round down: -3.2 to -2.8 m, grown to -3.7 to -2.3, columns -4 to -3.
    assert_eq!(visited(&grid, [-32, -2], [-28, 2]), [d]);
    // Points alone take cells of a meter: -0.4 to 0.4 m is columns and rows -1 to 0.
    grid.rebuild(at.map(|at| placed(&mut ids, at, 0)));
    assert_eq!(visited(&grid, [-4, -4], [4, 4]).len(), 1);

    // Bodies of 0.4 m to 2,048 m mixed, within 600 m of the origin: from a base of a meter, twice
    // each radius rounded up to a power of two of meters, 0.4 m takes level 0, 0.8 m level 1,
    // 3 m 3, 17 m 6, 120 m 8 and 2,048 m 12. Every box meets every body whose square overlaps it,
    // each once.
    let sizes = [4, 8, 30, 170, 1_200, 20_480];
    let bodies: Vec<_> = (0..60_i64)
        .map(|at| {
            let spread = |prime: i64| (at * prime).rem_euclid(12_001) - 6_000;
            let size = sizes[usize::try_from(at).unwrap() % sizes.len()];
            placed(&mut ids, [spread(7_919), spread(104_729)], size)
        })
        .collect();
    grid.rebuild(bodies.iter().copied());
    let levels: Vec<u8> = grid.widest.iter().map(|level| level.level).collect();
    assert_eq!(levels, [0, 1, 3, 6, 8, 12]);
    for (low, high) in [
        ([0, 0], [0, 0]),
        ([-3_000, 2_000], [-2_500, 2_600]),
        ([5_000, -6_000], [6_000, -5_000]),
        ([-20, -20], [20, 20]),
    ] {
        let got = visited(&grid, low, high);
        let overlaps = |entry: &&Indexed<(), u64>| {
            let (at, bound) = (entry.body.pos, entry.body.shape.bound());
            let near =
                |axis: Num, low: i64, high: i64| m(low) <= axis + bound && axis - bound <= m(high);
            near(at.x, low[0], high[0]) && near(at.z, low[1], high[1])
        };
        let missed = bodies
            .iter()
            .filter(overlaps)
            .find(|body| got.binary_search(&body.id).is_err());
        assert_eq!(missed, None, "{low:?} {high:?}");
        assert!(got.len() < bodies.len(), "{low:?} {high:?}");
    }
}

#[test]
fn a_box_meets_every_body_whose_square_overlaps_it() {
    let mut ids = 0;
    // A lattice 0.7 m apart from -5.6 to 5.6 m, radii 0 to 1.2 m: points make the base a meter,
    // and radii up to 0.5 m take its cells, up to 1 m cells of 2 m, and up to 1.2 m cells of 4 m.
    let mut lattice = Vec::new();
    for row in -8_i64..=8 {
        for column in -8..=8 {
            let radius = (row * 3 + column * 5).rem_euclid(13);
            lattice.push(placed(&mut ids, [column * 7, row * 7], radius));
        }
    }
    // Two points 50 km away either way spread the base's rows over 100,000, more than 4 times its
    // bodies and 64, so it keeps no row starts and searches all its bodies for each row.
    let far = [-500_000, 500_000].map(|z| placed(&mut ids, [0, z], 0));
    for outliers in [false, true] {
        let mut bodies = lattice.clone();
        if outliers {
            bodies.extend(far);
        }
        let mut grid = BodyGrid::default();
        grid.rebuild(bodies.iter().copied());
        let levels: Vec<u8> = grid.widest.iter().map(|level| level.level).collect();
        assert_eq!(levels, [0, 1, 2]);
        let kept = grid.rows.layers().next().unwrap().row(0);
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
            let overlaps = |entry: &&Indexed<(), u64>| {
                let at = entry.body.pos;
                let near = |axis: Num, low: i64, high: i64| {
                    m(low) <= axis + entry.body.shape.bound()
                        && axis - entry.body.shape.bound() <= m(high)
                };
                near(at.x, low[0], high[0]) && near(at.z, low[1], high[1])
            };
            for body in bodies.iter().filter(overlaps) {
                assert!(
                    got.binary_search(&body.id).is_ok(),
                    "{low:?} {high:?} {body:?}"
                );
            }
            // Exactly the bodies of the cells of their level that the box covers grown by the
            // level's widest body, 0.5, 1 and 1.2 m, whether row starts or searches find them.
            let inside = |entry: &&Indexed<(), u64>| {
                let (at, level) = (entry.body.pos, grid.levels.level(entry.body.shape.bound()));
                let grow = m([5, 10, 12][usize::from(level)]);
                let cell = |at: Num| grid.levels.index(level, at.to_bits());
                let within = |axis: Num, low: i64, high: i64| {
                    (cell(m(low) - grow)..=cell(m(high) + grow)).contains(&cell(axis))
                };
                within(at.x, low[0], high[0]) && within(at.z, low[1], high[1])
            };
            let mut expected: Vec<_> = bodies.iter().filter(inside).map(|body| body.id).collect();
            expected.sort_unstable();
            assert_eq!(got, expected, "{low:?} {high:?}");
        }
        // Columns past the cell keys' 28 bits clamp to their ends: a band of z -0.3 to 0.3 m
        // over every x covers rows -1 to 0 of the base, and is searched row by row, as it spans
        // fewer rows than the level has bodies; it finds each body of its rows, in every column.
        let mut got = Vec::new();
        grid.visit([Num::MIN, m(-3)], [Num::MAX, m(3)], |body| {
            got.push(body.id);
            ControlFlow::Continue(())
        });
        got.sort_unstable();
        let in_band = |entry: &&Indexed<(), u64>| {
            let level = grid.levels.level(entry.body.shape.bound());
            let grow = m([5, 10, 12][usize::from(level)]);
            let row = |at: Num| grid.levels.index(level, at.to_bits());
            (row(m(-3) - grow)..=row(m(3) + grow)).contains(&row(entry.body.pos.z))
        };
        let mut expected: Vec<_> = bodies.iter().filter(in_band).map(|body| body.id).collect();
        expected.sort_unstable();
        assert_eq!(got, expected, "outliers {outliers}");
    }
}
