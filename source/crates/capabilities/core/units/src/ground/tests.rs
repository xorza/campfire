use std::ops::Range;

use super::*;

/// `tenths` of a meter.
fn m(tenths: i64) -> Num {
    Num::from_int(tenths).unwrap() / 10
}

/// 3 × 3 samples 10 m apart, a step of 0.625 m, the first at `x = −10`, `z = −20`: the sample
/// `[u, v]` lies at `x = −10 + 10u`, `z = −20 + 10v`. Its heights, in steps and in meters:
///
/// ```text
/// v = 0:  0 16 32    0 10 20
/// v = 1:  0 16 32    0 10 20
/// v = 2:  0  0  0    0  0  0
/// ```
fn slope() -> HeightGrid {
    let step = Num::from_bits(5 << (Num::FRAC_BITS - 3));
    let samples = vec![0, 16, 32, 0, 16, 32, 0, 0, 0];
    HeightGrid::new([m(-100), m(-200)], m(100), step, 3, samples).unwrap()
}

/// 5 × 5 samples 10 m apart from the origin, a step of 1 m, all at 0 but the sample `[2, 1]` at
/// 100 m and `[4, 4]` at 7 m.
fn spike() -> HeightGrid {
    let mut samples = vec![0; 25];
    samples[5 + 2] = 100;
    samples[4 * 5 + 4] = 7;
    HeightGrid::new([Num::ZERO; 2], m(100), Num::ONE, 5, samples).unwrap()
}

#[test]
fn a_height_lies_on_the_triangle_of_the_maps_split_and_the_grids_edge_runs_on_past_it() {
    let at = |split, x, z| {
        let ground = Ground::new(slope(), split, None).unwrap();
        ground.height(m(x), m(z))
    };
    let [zero_two, one_three] = [CellSplit::ZeroTwo, CellSplit::OneThree];
    // The cell of column 0 and row 1 has corners of 0, 10, 0 and 0 m. At (−5, −7.5), its
    // fractions 0.5 and 0.25: split from corner 0 to 2, the lower triangle, 0 + 0.5 · 10 + 0.25 ·
    // (0 − 10) = 2.5; split from 1 to 3, the triangle of corner 0, 0 + 0.5 · 10 + 0.25 · 0 = 5.
    assert_eq!(at(zero_two, -50, -75), m(25));
    assert_eq!(at(one_three, -50, -75), m(50));
    // At (−2.5, −2.5), fractions 0.75 and 0.75: on the diagonal from 0 to 2, 0 + 7.5 + 0.75 · (0 −
    // 10) = 0; past the diagonal from 1 to 3, 0 + 0.25 · 0 + 0.25 · (10 − 0) = 2.5.
    assert_eq!(at(zero_two, -25, -25), Num::ZERO);
    assert_eq!(at(one_three, -25, -25), m(25));
    // On a sample, its height on either split: `[1, 1]`, at (0, −10), 10 m.
    assert_eq!(at(zero_two, 0, -100), m(100));
    assert_eq!(at(one_three, 0, -100), m(100));
    // Past the grid, the nearest point of its edge: east of it at `v = 1.5`, halfway from `[2,
    // 1]`'s 20 m to `[2, 2]`'s 0; beyond a corner, the corner's sample.
    assert_eq!(at(zero_two, 1000, -50), m(100));
    assert_eq!(at(one_three, 1000, -50), m(100));
    assert_eq!(at(zero_two, 1000, -1000), m(200));
    assert_eq!(at(zero_two, -1000, 1000), Num::ZERO);
    // A flat ground is at 0 everywhere.
    assert_eq!(Ground::default().height(m(5), m(-5)), Num::ZERO);
}

#[test]
fn a_height_rounds_once_to_the_nearest_even_bit() {
    // 2 × 2 samples 10 m apart, a step of 0.625 m, the sample `[1, 0]` one step up: on the row of
    // `z = 0`, the height at `x` bits is 0.625 · x ÷ 10 = x ÷ 16 bits. 1 bit gives 0.0625, 8 the
    // tie 0.5, 9 0.5625, 24 the tie 1.5 and 40 the tie 2.5: 0, 0, 1, 2 and 2.
    let step = Num::from_bits(5 << (Num::FRAC_BITS - 3));
    let grid = HeightGrid::new([Num::ZERO; 2], m(100), step, 2, vec![0, 1, 0, 0]).unwrap();
    let ground = Ground::new(grid, CellSplit::ZeroTwo, None).unwrap();
    let heights = [1, 8, 9, 24, 40].map(|bits| ground.height(Num::from_bits(bits), Num::ZERO));
    assert_eq!(heights, [0, 0, 1, 2, 2].map(Num::from_bits));
}

#[test]
fn the_walk_takes_bresenhams_cells_from_the_first_to_the_last() {
    let clipped = |start, end, counts| {
        let mut cells = Vec::new();
        Ground::walk(start, end, counts, |at| cells.push(at));
        cells
    };
    let walked = |start, end| clipped(start, end, [i64::MAX; 2]);
    // Shallow, x the major axis: the numerator starts at 3 / 2 = 1 and steps y as it reaches 3.
    assert_eq!(walked([0, 0], [3, 1]), [[0, 0], [1, 0], [2, 1], [3, 1]]);
    // Steep, y the major axis, by the same rule.
    assert_eq!(walked([0, 0], [1, 3]), [[0, 0], [0, 1], [1, 2], [1, 3]]);
    // Toward −x it is the shallow walk reversed, and toward −z its mirror.
    assert_eq!(walked([3, 1], [0, 0]), [[3, 1], [2, 1], [1, 0], [0, 0]]);
    assert_eq!(walked([0, 1], [3, 0]), [[0, 1], [1, 1], [2, 0], [3, 0]]);
    // A diagonal steps both axes each time, and one cell is itself.
    assert_eq!(walked([0, 0], [2, 2]), [[0, 0], [1, 1], [2, 2]]);
    assert_eq!(walked([2, 2], [2, 2]), [[2, 2]]);

    // Clipped to the major indices `0..counts`, a walk is the same cells, those outside left:
    // shallow over x `0..2`, steep over z `1..3` after its shift, and reversed over x `1..3`.
    let within = |cells: Vec<[i64; 2]>, axis: usize, range: Range<i64>| {
        cells
            .into_iter()
            .filter(|cell| range.contains(&cell[axis]))
            .collect::<Vec<_>>()
    };
    for (start, end, counts, major) in [
        ([-2, 0], [5, 3], [2, 9], 0),
        ([0, -1], [3, 6], [9, 3], 1),
        ([6, 2], [-1, 0], [3, 9], 0),
    ] {
        let unclipped = |shift: [i64; 2]| {
            let moved = |at: [i64; 2]| [at[0] + shift[0], at[1] + shift[1]];
            walked(moved(start), moved(end))
                .into_iter()
                .map(|at| [at[0] - shift[0], at[1] - shift[1]])
                .collect()
        };
        let expected = within(unclipped([10, 10]), major, 0..counts[major]);
        assert_eq!(
            clipped(start, end, counts),
            expected,
            "{start:?} to {end:?}"
        );
        assert!(!expected.is_empty());
    }
    // A line across the world over cells of one bit visits only the cells it can meet: 2⁴⁵ steps
    // long, it takes the 2 of `0..2`.
    let far = 1 << 45;
    assert_eq!(clipped([-far, 0], [far, 0], [2, 1]), [[0, 0], [1, 0]]);
}

#[test]
fn a_line_meets_the_highest_of_the_coarse_cells_it_crosses() {
    // With no span, the coarse cells are the grid's own: the cell `[i, j]` holds the samples `i`
    // to `i + 1` and `j` to `j + 1`, so the spike `[2, 1]` lies in the cells `[1, 0]`, `[2, 0]`,
    // `[1, 1]` and `[2, 1]`.
    let ground = Ground::new(spike(), CellSplit::ZeroTwo, None).unwrap();
    let along = |a: [i64; 2], b: [i64; 2]| ground.highest_along(a.map(m), b.map(m));
    // From the cell `[0, 0]` to `[3, 1]`: through `[1, 0]`, so 100 m.
    assert_eq!(along([50, 50], [350, 150]), Some(m(1000)));
    // From `[0, 0]` to `[1, 3]`, steep: `[0, 1]` and `[1, 2]`, which miss it, so 0.
    assert_eq!(along([50, 50], [150, 350]), Some(Num::ZERO));
    // From `[−2, 1]`, off the cells, to `[1, 1]`: the cells off them skipped, `[1, 1]` meets it;
    // a line off every cell meets none.
    assert_eq!(along([-150, 150], [150, 150]), Some(m(1000)));
    assert_eq!(along([-500, -500], [-600, -500]), None);
    // A flat ground's highest is 0.
    assert_eq!(
        Ground::default().highest_along([Num::ZERO; 2], [m(10); 2]),
        Some(Num::ZERO)
    );

    // Cells of 20 m from (40, 40) toward (0, 0): the cell `[0, 0]` spans x and z 20 to 40, so it
    // holds the samples 2 to 4 of each axis, `[4, 4]` among them, 7 m; `[1, 0]` spans x 0 to 20
    // and z 20 to 40, 0; `[0, 1]` and `[1, 1]` hold the spike.
    let span = CoarseSpan {
        cell: m(200),
        from: [m(400); 2],
        to: [Num::ZERO; 2],
    };
    let mirrored = Ground::new(spike(), CellSplit::ZeroTwo, Some(span)).unwrap();
    let along = |a: [i64; 2], b: [i64; 2]| mirrored.highest_along(a.map(m), b.map(m));
    assert_eq!(along([350, 350], [250, 300]), Some(m(70)));
    // A point on a cell's edge lies in the cell it starts, counted from `from`: x = 20 is the
    // cell `[1, 0]`'s, x = 21 still `[0, 0]`'s.
    assert_eq!(along([200, 350], [200, 350]), Some(Num::ZERO));
    assert_eq!(along([210, 350], [210, 350]), Some(m(70)));
    assert_eq!(along([350, 350], [350, 50]), Some(m(1000)));

    // Cells of 10 m from (40, 40) toward (60, 60), past the grid's far corner at (40, 40): each
    // holds the samples of the grid's edge it clamps to, the corner `[4, 4]`'s 7 m.
    let span = CoarseSpan {
        cell: m(100),
        from: [m(400); 2],
        to: [m(600); 2],
    };
    let past = Ground::new(spike(), CellSplit::ZeroTwo, Some(span)).unwrap();
    assert_eq!(past.highest_along([m(550); 2], [m(550); 2]), Some(m(70)));
    assert_eq!(
        past.highest_along([m(450); 2], [m(550), m(450)]),
        Some(m(70))
    );
}

#[test]
fn coarse_cells_are_no_smaller_than_the_grids_cells_nor_more_than_its_samples() {
    let span = |cell: i64, from: [i64; 2], to: [i64; 2]| {
        let span = CoarseSpan {
            cell: m(cell),
            from: from.map(m),
            to: to.map(m),
        };
        Ground::new(spike(), CellSplit::ZeroTwo, Some(span)).err()
    };
    // The spike's grid has 25 samples: 5 × 5 cells of 10 m are as many, 6 × 5 more. A span may
    // reach past the grid, as from (−1, 0).
    assert_eq!(span(100, [0, 0], [500, 500]), None);
    assert_eq!(span(100, [-10, 0], [400, 400]), None);
    assert_eq!(
        span(100, [0, 0], [600, 500]),
        Some(GroundError::CoarseCount)
    );
    assert_eq!(span(50, [0, 0], [400, 400]), Some(GroundError::CoarseCell));
    assert_eq!(span(100, [0, 0], [0, 400]), Some(GroundError::CoarseSpan));
    // A span past the world's bound, 2²⁰ m, at an end or by its cell.
    let past = Position::BOUND + Num::ONE;
    let beyond = |cell: Num, to: [Num; 2]| {
        let span = CoarseSpan {
            cell,
            from: [Num::ZERO; 2],
            to,
        };
        Ground::new(spike(), CellSplit::ZeroTwo, Some(span)).err()
    };
    assert_eq!(
        beyond(m(100), [past, m(400)]),
        Some(GroundError::CoarseBound)
    );
    assert_eq!(beyond(past, [m(400); 2]), Some(GroundError::CoarseBound));
    assert_eq!(beyond(Position::BOUND, [m(400); 2]), None);
}
