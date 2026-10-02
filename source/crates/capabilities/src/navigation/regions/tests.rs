use std::collections::BTreeMap;

use campfire_math::{Num, Vec3};

use super::*;
use crate::values::bounds::Bounds;

/// A grid of 1 m cells from the origin, `columns` by `rows`.
fn grid(columns: i64, rows: i64) -> Grid {
    let num = |value: i64| Num::from_int(value).unwrap();
    let bounds = Bounds::new([Num::ZERO; 2], [num(columns), num(rows)]).unwrap();
    Grid::new(Num::ONE, bounds).unwrap()
}

fn blocked_at(blocked: &[u64], cell: usize) -> bool {
    blocked[cell / 64] & 1 << (cell % 64) != 0
}

/// Cells blocked at random, about one in `odds`, from `seed`, by `SplitMix64`.
fn scatter(cells: usize, seed: u64, odds: u64) -> Vec<u64> {
    let mut state = seed;
    let mut blocked = vec![0; cells.div_ceil(64)];
    for cell in 0..cells {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        if (z ^ (z >> 31)).is_multiple_of(odds) {
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

/// Checks `regions` against a flood fill: two open cells share a reachable set exactly when
/// the flood fill puts them in one component; a blocked cell reaches its open side
/// neighbors' sets; the nearest reachable cell to a few goals is the nearest by brute force.
fn check(grid: &Grid, blocked: &[u64], regions: &Regions) {
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
