use std::collections::BTreeMap;

use campfire_math::{Num, Vec3};

use campfire_common::SegmentSeed;
use campfire_math::{RngSource, RngStream};

use super::*;
use crate::geometry::bounds::Bounds;

/// A grid of 1 m cells from the origin, `columns` by `rows`.
fn grid(columns: i64, rows: i64) -> Grid {
    let bounds = Bounds::new([Num::ZERO; 2], [Num::int(columns), Num::int(rows)]).unwrap();
    Grid::new(Num::ONE, bounds).unwrap()
}

fn blocked_at(blocked: &[u64], cell: usize) -> bool {
    blocked[cell / 64] & 1 << (cell % 64) != 0
}

/// Cells blocked at random, one in `odds` on average, from `seed`.
fn scatter(cells: usize, seed: u64, odds: u64) -> Vec<u64> {
    let source = RngSource::new(SegmentSeed::new([0; 32]));
    let mut rng = source.open(RngStream::new("scatter"), seed);
    let mut blocked = vec![0; cells.div_ceil(64)];
    for cell in 0..cells {
        if rng.below(odds) == 0 {
            blocked[cell / 64] |= 1 << (cell % 64);
        }
    }
    blocked
}

/// Each cell's component in a flood fill of the whole grid along sides; `None` when blocked.
fn flood(grid: &Grid, blocked: &[u64]) -> Vec<Option<usize>> {
    let (columns, rows) = (grid.columns(), grid.rows());
    let mut component = vec![None; columns * rows];
    for seed in 0..columns * rows {
        if component[seed].is_some() || blocked_at(blocked, seed) {
            continue;
        }
        component[seed] = Some(seed);
        let mut stack = vec![seed];
        while let Some(cell) = stack.pop() {
            let (x, z) = (cell % columns, cell / columns);
            let beside = [
                (x > 0).then(|| cell - 1),
                (x + 1 < columns).then(|| cell + 1),
                (z > 0).then(|| cell - columns),
                (z + 1 < rows).then(|| cell + columns),
            ];
            for next in beside.into_iter().flatten() {
                if component[next].is_none() && !blocked_at(blocked, next) {
                    component[next] = Some(seed);
                    stack.push(next);
                }
            }
        }
    }
    component
}

/// Checks each chunk's labels and region boxes against a flood fill of the chunk alone, from
/// each open cell not yet labeled, row by row: the regions are numbered in the order of their
/// first cell.
fn check_labels(blocked: &[u64], regions: &Regions) {
    let mut local = vec![0_u16; regions.local.len()];
    for chunk in 0..regions.chunks.len() {
        let [columns, rows] = regions.bounds(chunk);
        let mut boxes = Vec::new();
        for seed in rows.clone().flat_map(|row| {
            columns
                .clone()
                .map(move |column| row * regions.columns + column)
        }) {
            if local[seed] != 0 || blocked_at(blocked, seed) {
                continue;
            }
            boxes.push(([u32::MAX; 2], [0; 2]));
            let label = u16::try_from(boxes.len()).unwrap();
            local[seed] = label;
            let mut stack = vec![seed];
            while let Some(cell) = stack.pop() {
                let (x, z) = (cell % regions.columns, cell / regions.columns);
                let (low, high) = boxes.last_mut().unwrap();
                let at = [u32::try_from(x).unwrap(), u32::try_from(z).unwrap()];
                *low = [low[0].min(at[0]), low[1].min(at[1])];
                *high = [high[0].max(at[0]), high[1].max(at[1])];
                let beside = [
                    (x > columns.start).then(|| cell - 1),
                    (x + 1 < columns.end).then(|| cell + 1),
                    (z > rows.start).then(|| cell - regions.columns),
                    (z + 1 < rows.end).then(|| cell + regions.columns),
                ];
                for next in beside.into_iter().flatten() {
                    if local[next] == 0 && !blocked_at(blocked, next) {
                        local[next] = label;
                        stack.push(next);
                    }
                }
            }
        }
        let found: Vec<_> = regions.chunks[chunk]
            .regions
            .iter()
            .map(|region| (region.low, region.high))
            .collect();
        assert_eq!(found, boxes, "chunk {chunk}");
    }
    assert_eq!(regions.local, local);
}

/// Checks `regions` against a flood fill: each chunk's labels and boxes are a flood fill's of
/// the chunk; two open cells share a reachable set exactly when
/// the flood fill puts them in one component; a blocked cell reaches its open side
/// neighbors' sets; the nearest reachable cell to a few goals is the nearest by brute force.
fn check(grid: &Grid, blocked: &[u64], regions: &Regions) {
    check_labels(blocked, regions);
    let component = flood(grid, blocked);
    let mut by_set = BTreeMap::new();
    let mut by_component = BTreeMap::new();
    for (cell, &found) in component.iter().enumerate() {
        let set = regions.set(cell);
        assert_eq!(set.is_some(), found.is_some(), "cell {cell}");
        if let (Some(set), Some(found)) = (set, found) {
            assert_eq!(*by_set.entry(set).or_insert(found), found, "cell {cell}");
            assert_eq!(
                *by_component.entry(found).or_insert(set),
                set,
                "cell {cell}"
            );
        }
    }
    let mut candidates = Vec::new();
    let columns = grid.columns();
    for (index, cell) in (0..component.len()).step_by(97).enumerate() {
        let reach = regions.reach(cell);
        if reach.is_empty() {
            continue;
        }
        let quarter =
            |value: usize| Num::from_bits(i64::try_from(value).unwrap() << (Num::FRAC_BITS - 2));
        let goal = Position::new(Vec3::new(
            quarter(index * 37 % (4 * columns)),
            Num::ZERO,
            quarter(index * 53 % (4 * grid.rows())),
        ))
        .unwrap();
        let brute = (0..component.len())
            .filter(|&other| regions.set(other).is_some_and(|set| reach.contains(set)))
            .min_by_key(|&other| (grid.center_distance(other, goal), other));
        assert_eq!(
            regions.nearest(grid, reach, goal, &mut candidates, &mut 0),
            brute
        );
    }
}

#[test]
fn regions_join_exactly_the_cells_a_flood_fill_joins() {
    // A wall down column 70, in the second chunk, with a gap at row 80, in the second chunk
    // row: the left and right parts meet only through the gap, across chunk sides.
    let walled = grid(150, 90);
    let mut blocked = vec![0; walled.cells().div_ceil(64)];
    for row in (0..90).filter(|&row| row != 80) {
        let cell = row * 150 + 70;
        blocked[cell / 64] |= 1 << (cell % 64);
    }
    let regions = Regions::new(&walled, &blocked);
    check(&walled, &blocked, &regions);
    assert!(regions.reach(0).meets(regions.reach(149)));
    let gap = 80 * 150 + 70;
    blocked[gap / 64] |= 1 << (gap % 64);
    let closed = Regions::new(&walled, &blocked);
    assert!(!closed.reach(0).meets(closed.reach(149)));
    // The wall cell at row 0 is blocked; it reaches both sides, as a walker may leave it
    // either way.
    assert!(closed.reach(70).meets(closed.reach(0)));
    assert!(closed.reach(70).meets(closed.reach(149)));

    // Scattered cells, sparse to dense, on grids that end inside a chunk.
    for (seed, odds) in [(1, 9), (2, 4), (3, 3), (4, 2)] {
        let scattered = grid(150, 90);
        let blocked = scatter(scattered.cells(), seed, odds);
        check(&scattered, &blocked, &Regions::new(&scattered, &blocked));
    }

    // Shapes smaller than a chunk, a line either way, whole chunks and one a cell past them,
    // each all open, all blocked, a checkerboard and scattered. A checkerboard isolates each
    // open cell: 2048 regions in a chunk, and a blocked cell beside four sets, each a place of
    // its reach.
    let shapes = [(1, 1), (1, 130), (130, 1), (64, 64), (128, 64), (65, 63)];
    for (columns, rows) in shapes {
        let shape = grid(columns, rows);
        let cells = shape.cells();
        let columns = usize::try_from(columns).unwrap();
        let fill = |blocks: &dyn Fn(usize) -> bool| {
            let mut blocked = vec![0; cells.div_ceil(64)];
            for cell in (0..cells).filter(|&cell| blocks(cell)) {
                blocked[cell / 64] |= 1 << (cell % 64);
            }
            blocked
        };
        let patterns = [
            fill(&|_| false),
            fill(&|_| true),
            fill(&|cell| (cell % columns + cell / columns) % 2 == 1),
            scatter(cells, 5, 3),
        ];
        for blocked in patterns {
            check(&shape, &blocked, &Regions::new(&shape, &blocked));
        }
    }
}

#[test]
fn a_rebuild_labels_again_only_the_chunks_it_is_told_changed() {
    let grid = grid(150, 90);
    let mut blocked = scatter(grid.cells(), 5, 4);
    let mut regions = Regions::new(&grid, &blocked);
    // Toggle cells in chunk 0, at (3, 3), and in chunk 4, at (70, 70), marking both.
    let mut dirty = vec![false; regions.chunks()];
    for cell in [3 * 150 + 3, 70 * 150 + 70] {
        blocked[cell / 64] ^= 1 << (cell % 64);
        regions.touch(cell..cell + 1, &mut dirty);
    }
    assert_eq!(
        dirty,
        [true, false, false, false, true, false],
        "chunks 0 and 4 of 3 × 2"
    );
    regions.rebuild(&blocked, &dirty);
    let fresh = Regions::new(&grid, &blocked);
    assert_eq!(
        (&regions.local, &regions.chunks),
        (&fresh.local, &fresh.chunks)
    );
    check(&grid, &blocked, &regions);

    // Rounds of cells toggled in a few chunks, corners and sides among them, each rebuilt
    // from the chunks touched alone: the regions and the pairs along every side are those
    // a fresh build makes, which the flood fill checks.
    let mut state = 11_u64;
    for round in 0..12 {
        let mut dirty = vec![false; regions.chunks()];
        for _ in 0..=round % 4 {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            let cell = usize::try_from(state >> 33).unwrap() % grid.cells();
            blocked[cell / 64] ^= 1 << (cell % 64);
            regions.touch(cell..cell + 1, &mut dirty);
        }
        regions.rebuild(&blocked, &dirty);
        let fresh = Regions::new(&grid, &blocked);
        assert_eq!(
            (&regions.local, &regions.chunks),
            (&fresh.local, &fresh.chunks),
            "round {round}"
        );
    }

    // A cell toggled in chunk 0 while only chunk 1 is marked keeps chunk 0's old labels.
    let cell = 10 * 150 + 10;
    let before = regions.local[cell];
    blocked[cell / 64] ^= 1 << (cell % 64);
    let mut dirty = vec![false; regions.chunks()];
    dirty[1] = true;
    regions.rebuild(&blocked, &dirty);
    assert_eq!(regions.local[cell], before);
}
