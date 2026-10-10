use campfire_math::internals::SplitMix64;

use super::*;

fn at(x: Num, z: Num) -> Position {
    Position::new(Vec3::new(x, Num::ZERO, z)).unwrap()
}

/// The cells `Grid::touches` visits from `from` to `to`, each column's rows found by dividing
/// its span's ends, as the walk did before it carried its quotient.
fn touched_by_division(grid: &Grid, from: Position, to: Position) -> Vec<usize> {
    let min = grid.bounds.min().map(|axis| i128::from(axis.to_bits()));
    let ground = |pos: Position| {
        let at = pos.get();
        [
            i128::from(at.x.to_bits()) - min[0],
            i128::from(at.z.to_bits()) - min[1],
        ]
    };
    let (mut a, mut b) = (ground(from), ground(to));
    if a[0] > b[0] {
        (a, b) = (b, a);
    }
    let cell = i128::from(grid.cell.to_bits());
    let last = |axis: usize| i128::from(grid.size[axis]) - 1;
    let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
    let mut cells = Vec::new();
    for column in (ceil_div(a[0], cell) - 1).max(0)..=b[0].div_euclid(cell).min(last(0)) {
        let (z_low, z_high, scale) = if dx == 0 {
            (a[1].min(b[1]), a[1].max(b[1]), cell)
        } else {
            let at = |x: i128| a[1] * dx + (x - a[0]) * dz;
            let start = at(a[0].max(column * cell));
            let end = at(b[0].min((column + 1) * cell));
            (start.min(end), start.max(end), dx * cell)
        };
        for row in (ceil_div(z_low, scale) - 1).max(0)..=z_high.div_euclid(scale).min(last(1)) {
            let at = row * i128::from(grid.size[0]) + column;
            cells.push(usize::try_from(at).unwrap());
        }
    }
    cells
}

#[test]
fn a_grid_covers_its_rectangle_in_whole_cells_and_reveals_exactly() {
    // 1 m cells over (−2, −1) to (2, 1.5): 4 along x, 3 along z, the last row half outside.
    let half = Num::HALF;
    let bounds = Bounds::new(
        [Num::int(-2), Num::int(-1)],
        [Num::int(2), Num::int(1) + half],
    )
    .unwrap();
    let grid = Grid::new(Num::int(1), bounds).unwrap();
    assert_eq!(grid.cells(), 12);
    // Cell (2, 1) is x from 0 to 1, z from 0 to 1: number 1 × 4 + 2 = 6.
    assert_eq!(grid.cell_of(at(Num::ZERO, Num::ZERO)), Some(6));
    assert_eq!(
        grid.cell_of(at(Num::int(1) - Num::EPSILON, Num::int(1) - Num::EPSILON)),
        Some(6)
    );
    assert_eq!(grid.cell_of(at(Num::int(-2), Num::int(-1))), Some(0));
    // On the max edges: x = 2 ends cell column 3, the last, so (2, 0) is in 1 × 4 + 3 = 7;
    // z = 1.5 is inside row 2, so (0, 1.5) is in 2 × 4 + 2 = 10, and the corner in 11.
    let edges = [
        at(Num::int(2), Num::ZERO),
        at(Num::ZERO, Num::int(1) + half),
        at(Num::int(2), Num::int(1) + half),
    ];
    assert_eq!(
        edges.map(|pos| grid.cell_of(pos)),
        [Some(7), Some(10), Some(11)]
    );
    // Outside the bounds, even within the last row's cells, which reach z = 2.
    let off = [
        at(Num::int(2) + Num::EPSILON, Num::ZERO),
        at(Num::int(-2) - Num::EPSILON, Num::ZERO),
        at(Num::ZERO, Num::int(1) + half + Num::EPSILON),
    ];
    assert_eq!(off.map(|pos| grid.cell_of(pos)), [None, None, None]);

    // From (0, 0) the nearest centers, (±0.5, ±0.5), are √0.5 ≈ 0.707 m away, the next
    // ones, such as (1.5, 0.5), √2.5 ≈ 1.58 m. A radius of 1.5 reaches only the four.
    let reveal = |radius: Num| {
        let mut cells = Vec::new();
        grid.spans_within(at(Num::ZERO, Num::ZERO), radius, |span| cells.extend(span));
        cells
    };
    assert_eq!(reveal(Num::int(1) + half), [1, 2, 5, 6]);
    // A radius of 1.59 reaches the centers √2.5 ≈ 1.581 away too: the ring around the four,
    // less the cells off the grid and the corners, √4.5 away.
    let reach = Num::from_bits((159 << 24) / 100);
    assert_eq!(reveal(reach), [0, 1, 2, 3, 4, 5, 6, 7, 9, 10]);
    assert!(reveal(Num::ZERO).is_empty());
    // A radius past every center, the largest number among them, reveals every cell: the
    // reach stops at 8 bounds before it doubles.
    let every: Vec<usize> = (0..grid.cells()).collect();
    assert_eq!(reveal(Num::MAX), every);
    assert_eq!(reveal(Position::BOUND), every);

    // Against each cell's center tested alone, in halves of a bit, from points on and off
    // the grid, on cell lines and between them, with radii that end on centers and between.
    let quarter = Num::QUARTER;
    let twice = |value: Num| 2 * i128::from(value.to_bits());
    for x in -12..12 {
        for z in -8..10 {
            let pos = at(quarter * x, quarter * z);
            for radius in [0, 1, 2, 3, 5, 6, 7, 9, 12, 20].map(|r| quarter * r) {
                // Within the radius, and strictly closer than it.
                for strict in [false, true] {
                    let mut spans = Vec::new();
                    if strict {
                        grid.spans_closer(pos, radius, |span| spans.extend(span));
                    } else {
                        grid.spans_within(pos, radius, |span| spans.extend(span));
                    }
                    let alone: Vec<usize> = (0..grid.cells())
                        .filter(|&cell| {
                            let center = |index: usize, axis: usize| {
                                twice(grid.bounds.min()[axis])
                                    + i128::from(grid.cell.to_bits()) * (2 * index as i128 + 1)
                            };
                            let dx = twice(pos.get().x) - center(cell % 4, 0);
                            let dz = twice(pos.get().z) - center(cell / 4, 1);
                            let square = dx * dx + dz * dz;
                            let reach = twice(radius) * twice(radius);
                            if strict {
                                square < reach
                            } else {
                                square <= reach
                            }
                        })
                        .collect();
                    assert_eq!(spans, alone, "{x} {z} {radius:?} {strict}");
                }
            }
        }
    }

    // 2048 × 2049 cells are more than 2²² = 2048 × 2048.
    let wide = Bounds::new([Num::int(0), Num::int(0)], [Num::int(2048), Num::int(2049)]).unwrap();
    let square = Bounds::new([Num::int(0), Num::int(0)], [Num::int(2048), Num::int(2048)]).unwrap();
    assert_eq!(
        Grid::new(Num::int(1), square).map(|grid| grid.cells()),
        Some(1 << 22)
    );
    for (cell, bounds) in [
        (Num::ZERO, bounds),
        (Position::BOUND + Num::EPSILON, bounds),
        (Num::int(1), wide),
    ] {
        assert_eq!(Grid::new(cell, bounds), None, "{cell:?} {bounds:?}");
    }
}

#[test]
fn the_rows_in_lanes_are_the_rows_one_by_one() {
    // The 3v3's map, (−48, −68) to (48, 68), in the fog's 1 m cells and the pathing grid's half
    // meters. Discs centered on and off the grid, on cell lines and between, of radii from 0 to
    // the gate's last, within and strictly closer; and the gate's edges.
    let bounds = Bounds::new([Num::int(-48), Num::int(-68)], [Num::int(48), Num::int(68)]).unwrap();
    let mut words = SplitMix64::new(0x5EED);
    for cell in [Num::ONE, Num::HALF] {
        let grid = Grid::new(cell, bounds).unwrap();
        let bits = cell.to_bits();
        // A reach of 2·radius halves fits while 2·radius + 8·cell < 2³¹.
        let widest = (1 << 30) - 4 * bits - 1;
        let rows_of =
            |radius: i64| grid.disc_rows(at(Num::ZERO, Num::ZERO), Num::from_bits(radius), false);
        assert!(rows_of(widest).fit_lanes(bits));
        assert!(!rows_of(widest + 1).fit_lanes(bits));
        let runs = |rows: &Rows, lanes: bool| {
            let mut runs = Vec::new();
            let mut keep = |run: Range<usize>| runs.push(run);
            if lanes {
                grid.rows_in_lanes(rows, &mut keep);
            } else {
                grid.rows_one_by_one(rows, &mut keep);
            }
            runs
        };
        let span = 120 * Num::ONE.to_bits().cast_unsigned();
        for case in 0..4000_u64 {
            let mut point = || {
                let along = (words.next_u64() % span).cast_signed() - 60 * Num::ONE.to_bits();
                // One case in three on a line between cells or through their centers.
                if case % 3 == 0 {
                    along - along % (bits / 2)
                } else {
                    along
                }
            };
            let pos = at(Num::from_bits(point()), Num::from_bits(point()));
            let radius = match case % 4 {
                0 => widest - (words.next_u64() % 4).cast_signed(),
                1 => (words.next_u64() % (4 * Num::ONE.to_bits().cast_unsigned())).cast_signed(),
                _ => (words.next_u64() % widest.cast_unsigned()).cast_signed(),
            };
            for strict in [false, true] {
                let rows = grid.disc_rows(pos, Num::from_bits(radius), strict);
                assert!(rows.fit_lanes(bits));
                assert_eq!(
                    runs(&rows, true),
                    runs(&rows, false),
                    "{pos:?} {radius} {strict}"
                );
            }
        }
    }
}

#[test]
fn a_segment_touches_the_cells_whose_closed_squares_it_meets() {
    // 1 m cells over (0, 0) to (4, 4), numbered 4 × row + column; points in halves of a meter.
    let bounds = Bounds::new([Num::ZERO; 2], [Num::int(4); 2]).unwrap();
    let grid = Grid::new(Num::ONE, bounds).unwrap();
    let half = |value: i64| Num::HALF * value;
    let touched = |from: [i64; 2], to: [i64; 2]| {
        let mut cells = Vec::new();
        let point = |[x, z]: [i64; 2]| at(half(x), half(z));
        grid.touches(point(from), point(to), |cell| {
            cells.push(cell);
            false
        });
        cells
    };
    // Along row 0's middle from column 0 to 2.
    assert_eq!(touched([1, 1], [5, 1]), [0, 1, 2]);
    // A diagonal through the corners (1, 1) and (2, 2), column by column: each corner touches the
    // four cells round it.
    assert_eq!(touched([1, 1], [5, 5]), [0, 4, 1, 5, 9, 6, 10]);
    // Up the line x = 2 between columns 1 and 2, from z = 0.5 to 1.5.
    assert_eq!(touched([4, 1], [4, 3]), [1, 5, 2, 6]);
    // A point in a cell, and one on a corner.
    assert_eq!(touched([3, 3], [3, 3]), [5]);
    assert_eq!(touched([4, 4], [4, 4]), [5, 9, 6, 10]);
    // From (0.5, 0.5) to (3.5, 1.5), a third of a meter up a meter along: z is 2/3 at x = 1, 1
    // at x = 2, on the line between rows 0 and 1, and 4/3 at x = 3; either way round.
    assert_eq!(touched([1, 1], [7, 3]), [0, 1, 5, 2, 6, 7]);
    assert_eq!(touched([7, 3], [1, 1]), [0, 1, 5, 2, 6, 7]);
    // On grids of 1 m, half-meter and 0.3 m cells, whose last column and row reach past the
    // bounds, segments of every slope, on and off the lines between cells, and points, within
    // the grid and past each of its sides, visit the cells that dividing each column's span
    // gives, in its order.
    let mut words = SplitMix64::new(0x70C4);
    for cell in [
        Num::ONE,
        Num::HALF,
        Num::from_bits(Num::ONE.to_bits() * 3 / 10),
    ] {
        let bounds = Bounds::new([Num::int(-7), Num::int(-5)], [Num::int(9), Num::int(6)]).unwrap();
        let grid = Grid::new(cell, bounds).unwrap();
        let bits = cell.to_bits().cast_unsigned();
        for case in 0..2000 {
            let mut along = |low: i64, high: i64| {
                let span = (high - low).cast_unsigned() * Num::ONE.to_bits().cast_unsigned();
                let offset = words.next_u64() % (span + 1);
                // One point in four on a line between cells.
                let offset = if case % 4 == 0 {
                    offset - offset % bits
                } else {
                    offset
                };
                Num::int(low) + Num::from_bits(offset.cast_signed())
            };
            let from = at(along(-12, 14), along(-9, 10));
            let to = match case % 5 {
                0 => from,
                1 => at(from.get().x, along(-5, 6)),
                _ => at(along(-12, 14), along(-9, 10)),
            };
            let mut cells = Vec::new();
            grid.touches(from, to, |cell| {
                cells.push(cell);
                false
            });
            assert_eq!(
                cells,
                touched_by_division(&grid, from, to),
                "{from:?} {to:?}"
            );
        }
    }

    // The visit ends at the first hit.
    let mut seen = 0;
    assert!(
        grid.touches(at(half(1), half(1)), at(half(5), half(1)), |cell| {
            seen += 1;
            cell == 1
        })
    );
    assert_eq!(seen, 2);
}

/// The cells of `grid` that `test` keeps, from the runs `spans` gives, which must be in rows in
/// order, each run of one row, none empty.
fn from_runs(grid: &Grid, spans: impl FnOnce(&mut dyn FnMut(Range<usize>))) -> Vec<usize> {
    let mut cells = Vec::new();
    let mut last_row = None;
    spans(&mut |run| {
        assert!(!run.is_empty());
        let row = run.start / grid.columns();
        assert_eq!(
            (run.end - 1) / grid.columns(),
            row,
            "a run stays in its row"
        );
        assert!(
            last_row.is_none_or(|last| last < row),
            "one run a row, rows in order"
        );
        last_row = Some(row);
        cells.extend(run);
    });
    cells
}

#[test]
fn a_box_marks_exactly_the_cells_it_comes_closer_to_and_those_it_covers() {
    // Cells of 0.3 m over ±6 m, which puts centres at odd halves of a bit's grid; boxes at any
    // angle and place, some by the bounds' edge: each run is a cell a brute test of every cell
    // keeps, and none is missed.
    let bounds = Bounds::new([Num::int(-6); 2], [Num::int(6); 2]).unwrap();
    let cell = Num::from_bits(3 * (1 << Num::FRAC_BITS) / 10);
    let grid = Grid::new(cell, bounds).unwrap();
    let mut words = SplitMix64::new(0xB0E5);
    let mut draw =
        |low: i64, span: i64| low + i64::try_from(words.next_u64() % span.cast_unsigned()).unwrap();
    let one = Num::ONE.to_bits();
    let (mut marked, mut covered) = (0, 0);
    for _ in 0..40 {
        let size = [draw(one / 4, 4 * one), draw(one / 4, 4 * one)].map(Num::from_bits);
        let body = BodyBox::new(size, Num::from_bits(draw(0, 360 * one))).unwrap();
        let centre = at(
            Num::from_bits(draw(-6 * one, 12 * one)),
            Num::from_bits(draw(-6 * one, 12 * one)),
        );
        let reach = Num::from_bits(draw(0, one));
        let off = |cell: usize| grid.center_twice(cell) - Halves::ground(centre);
        let closer = from_runs(&grid, |mark| {
            grid.box_spans_closer(centre, &body, reach, mark);
        });
        let expected: Vec<usize> = (0..grid.cells())
            .filter(|&cell| body.closer_twice(off(cell), reach))
            .collect();
        assert_eq!(closer, expected, "{body:?} at {centre:?} within {reach:?}");
        let half = i128::from(cell.to_bits());
        let covers = from_runs(&grid, |mark| grid.box_covers(centre, &body, mark));
        let expected: Vec<usize> = (0..grid.cells())
            .filter(|&cell| body.overlaps_square_twice(off(cell), half))
            .collect();
        assert_eq!(covers, expected, "{body:?} at {centre:?}");
        marked += closer.len();
        covered += covers.len();
    }
    assert!(marked > 1000 && covered > 1000, "{marked} {covered}");
}
