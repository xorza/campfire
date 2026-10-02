use std::mem;
use std::ops::Range;

use campfire_sim::Position;

use crate::values::grid::Grid;

/// The cells of one layer of the pathing grid a walker can go between, as 0 A.D.'s hierarchical
/// pathfinder keeps them: the grid in chunks of `CHUNK` × `CHUNK` cells, each chunk's open cells
/// split into regions of cells that touch along a side, regions of chunks side by side joined where
/// their cells touch, and the joined regions numbered as reachable sets. Touching along a side is
/// exact for the moves the planner takes: a diagonal step needs both cells beside it open. A
/// build labels again only the chunks it is told changed, then joins every region again, which
/// costs the regions and the cells along chunk sides, not the cells. Derived from the layer, not
/// state.
#[derive(Debug)]
pub(crate) struct Regions {
    columns: usize,
    rows: usize,
    chunk_columns: usize,
    /// Each cell's region in its chunk, counted from 1; 0 for a blocked cell.
    local: Vec<u16>,
    /// The regions, chunk after chunk; chunk `c`'s are `starts[c]..starts[c + 1]`.
    all: Vec<Region>,
    starts: Vec<u32>,
    /// The regions and starts a build makes, swapped in when it ends, kept between builds.
    spare: Vec<Region>,
    spare_starts: Vec<u32>,
    /// The cells a flood fill has yet to spread from, and each region's parent as regions join.
    stack: Vec<usize>,
    parents: Vec<u32>,
}

/// A region of a chunk: the box of its cells, columns and rows from `low` to `high` both in, and
/// its reachable set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Region {
    low: [u32; 2],
    high: [u32; 2],
    set: u32,
}

/// The reachable sets a cell belongs to: its own region's, or for a blocked cell, which a walker
/// may leave or enter, those of the open cells beside it; the first `count`, ascending.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Reach {
    sets: [u32; 4],
    count: u8,
}

/// A region the nearest cell to a goal may lie in: by how near its box comes to the goal, in
/// squares of half bits, then by its number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Candidate {
    bound: u128,
    region: u32,
}

impl Regions {
    /// The side of a chunk in cells: a flood fill a change starts spans at most 4096 cells.
    const CHUNK: usize = 64;

    /// The regions of the layer whose blocked cells `blocked` marks, one bit a cell, on `grid`.
    pub(crate) fn new(grid: &Grid, blocked: &[u64]) -> Regions {
        let (columns, rows) = (grid.columns(), grid.rows());
        let chunk_columns = columns.div_ceil(Regions::CHUNK);
        let chunks = chunk_columns * rows.div_ceil(Regions::CHUNK);
        let mut regions = Regions {
            columns,
            rows,
            chunk_columns,
            local: vec![0; columns * rows],
            all: Vec::new(),
            starts: vec![0; chunks + 1],
            spare: Vec::new(),
            spare_starts: Vec::with_capacity(chunks + 1),
            stack: Vec::new(),
            parents: Vec::new(),
        };
        regions.build(blocked, |_| true);
        regions
    }

    /// Labels again the chunks `dirty` marks, from `blocked`, then joins every region again.
    pub(crate) fn rebuild(&mut self, blocked: &[u64], dirty: &[bool]) {
        debug_assert_eq!(dirty.len(), self.starts.len() - 1);
        self.build(blocked, |chunk| dirty[chunk]);
    }

    /// The number of chunks, for a caller that marks those a change touched.
    pub(crate) const fn chunks(&self) -> usize {
        self.starts.len() - 1
    }

    /// Marks in `dirty` the chunks the run `cells` of one row lies in.
    pub(crate) const fn touch(&self, cells: Range<usize>, dirty: &mut [bool]) {
        if cells.start >= cells.end {
            return;
        }
        let row = cells.start / self.columns;
        let first = cells.start % self.columns / Regions::CHUNK;
        let last = (cells.end - 1) % self.columns / Regions::CHUNK;
        let base = row / Regions::CHUNK * self.chunk_columns;
        let mut chunk = first;
        while chunk <= last {
            dirty[base + chunk] = true;
            chunk += 1;
        }
    }

    /// The reachable sets `cell` belongs to.
    pub(crate) fn reach(&self, cell: usize) -> Reach {
        let mut reach = Reach {
            sets: [0; 4],
            count: 0,
        };
        if let Some(set) = self.set(cell) {
            reach.add(set);
            return reach;
        }
        let (column, row) = (cell % self.columns, cell / self.columns);
        let beside = [
            (column > 0).then(|| cell - 1),
            (column + 1 < self.columns).then(|| cell + 1),
            (row > 0).then(|| cell - self.columns),
            (row + 1 < self.rows).then(|| cell + self.columns),
        ];
        for set in beside
            .into_iter()
            .flatten()
            .filter_map(|cell| self.set(cell))
        {
            reach.add(set);
        }
        reach
    }

    /// The cell of a region in `reach` whose center on `grid` is nearest `goal`, ties to the lower
    /// number; `None` when `reach` is empty. Regions go in order of how near their box comes to
    /// the goal, and each is scanned until a box comes no nearer than the best cell found.
    /// `candidates` is a buffer the caller keeps; each region and each cell scanned counts in
    /// `work`.
    pub(crate) fn nearest(
        &self,
        grid: &Grid,
        reach: Reach,
        goal: Position,
        candidates: &mut Vec<Candidate>,
        work: &mut u32,
    ) -> Option<usize> {
        candidates.clear();
        candidates.extend(
            self.all
                .iter()
                .enumerate()
                .filter(|(_, region)| reach.contains(region.set))
                .map(|(number, region)| Candidate {
                    bound: grid.box_distance(region.low.map(widen), region.high.map(widen), goal),
                    region: u32::try_from(number).expect("regions fit their cells"),
                }),
        );
        candidates.sort_unstable();
        *work += u32::try_from(self.all.len()).expect("regions fit their cells");
        let mut best: Option<(u128, usize)> = None;
        for candidate in &*candidates {
            if best.is_some_and(|(distance, _)| candidate.bound > distance) {
                break;
            }
            let region = self.all[candidate.region as usize];
            let chunk = self
                .starts
                .partition_point(|&start| start <= candidate.region)
                - 1;
            let label = candidate.region - self.starts[chunk] + 1;
            let [columns, rows] = [0, 1].map(|axis| region.high[axis] - region.low[axis] + 1);
            *work += columns * rows;
            for row in widen(region.low[1])..=widen(region.high[1]) {
                for column in widen(region.low[0])..=widen(region.high[0]) {
                    let cell = row * self.columns + column;
                    if u32::from(self.local[cell]) != label {
                        continue;
                    }
                    let found = (grid.center_distance(cell, goal), cell);
                    if best.is_none_or(|best| found < best) {
                        best = Some(found);
                    }
                }
            }
        }
        best.map(|(_, cell)| cell)
    }

    /// The reachable set of open `cell`; `None` for a blocked one.
    fn set(&self, cell: usize) -> Option<u32> {
        let label = self.local[cell];
        if label == 0 {
            return None;
        }
        let chunk = self.chunk_of(cell);
        let number = self.starts[chunk] + u32::from(label) - 1;
        Some(self.all[number as usize].set)
    }

    const fn chunk_of(&self, cell: usize) -> usize {
        let (column, row) = (cell % self.columns, cell / self.columns);
        row / Regions::CHUNK * self.chunk_columns + column / Regions::CHUNK
    }

    /// The columns and rows of `chunk`, each from its first to past its last.
    fn bounds(&self, chunk: usize) -> [Range<usize>; 2] {
        let column = chunk % self.chunk_columns * Regions::CHUNK;
        let row = chunk / self.chunk_columns * Regions::CHUNK;
        [
            column..(column + Regions::CHUNK).min(self.columns),
            row..(row + Regions::CHUNK).min(self.rows),
        ]
    }

    /// Labels again each chunk `relabel` names, keeps the regions of the others, and joins them.
    fn build(&mut self, blocked: &[u64], relabel: impl Fn(usize) -> bool) {
        let mut spare = mem::take(&mut self.spare);
        let mut spare_starts = mem::take(&mut self.spare_starts);
        spare.clear();
        spare_starts.clear();
        for chunk in 0..self.chunks() {
            spare_starts.push(u32::try_from(spare.len()).expect("regions fit their cells"));
            if relabel(chunk) {
                self.label(chunk, blocked, &mut spare);
            } else {
                let kept = self.starts[chunk] as usize..self.starts[chunk + 1] as usize;
                spare.extend_from_slice(&self.all[kept]);
            }
        }
        spare_starts.push(u32::try_from(spare.len()).expect("regions fit their cells"));
        self.spare = mem::replace(&mut self.all, spare);
        self.spare_starts = mem::replace(&mut self.starts, spare_starts);
        self.join();
    }

    /// Splits the open cells of `chunk` into regions by flood fill along sides, numbered in the
    /// order of their first cell, row by row, and pushes them to `regions`.
    fn label(&mut self, chunk: usize, blocked: &[u64], regions: &mut Vec<Region>) {
        let open = |cell: usize| blocked[cell / 64] & 1 << (cell % 64) == 0;
        let [columns, rows] = self.bounds(chunk);
        for row in rows.clone() {
            let first = row * self.columns;
            self.local[first + columns.start..first + columns.end].fill(0);
        }
        let mut label = 0;
        for row in rows.clone() {
            for column in columns.clone() {
                let seed = row * self.columns + column;
                if self.local[seed] != 0 || !open(seed) {
                    continue;
                }
                label += 1;
                let narrow =
                    |value: usize| u32::try_from(value).expect("a grid has at most 2²² cells");
                let mut region = Region {
                    low: [narrow(column), narrow(row)],
                    high: [narrow(column), narrow(row)],
                    set: 0,
                };
                self.local[seed] = label;
                self.stack.push(seed);
                while let Some(cell) = self.stack.pop() {
                    let (x, z) = (cell % self.columns, cell / self.columns);
                    region.low = [region.low[0].min(narrow(x)), region.low[1].min(narrow(z))];
                    region.high = [region.high[0].max(narrow(x)), region.high[1].max(narrow(z))];
                    let beside = [
                        (x > columns.start).then(|| cell - 1),
                        (x + 1 < columns.end).then(|| cell + 1),
                        (z > rows.start).then(|| cell - self.columns),
                        (z + 1 < rows.end).then(|| cell + self.columns),
                    ];
                    for next in beside.into_iter().flatten() {
                        if self.local[next] == 0 && open(next) {
                            self.local[next] = label;
                            self.stack.push(next);
                        }
                    }
                }
                regions.push(region);
            }
        }
    }

    /// Joins the regions whose cells touch across a chunk's right or lower side, and gives each
    /// region the lowest number of the regions joined to it as its reachable set.
    fn join(&mut self) {
        let count = self.all.len();
        self.parents.clear();
        self.parents
            .extend((0..count).map(|number| u32::try_from(number).expect("regions fit")));
        for chunk in 0..self.chunks() {
            let [columns, rows] = self.bounds(chunk);
            if columns.end < self.columns {
                for row in rows.clone() {
                    let cell = row * self.columns + columns.end - 1;
                    self.unite(cell, cell + 1);
                }
            }
            if rows.end < self.rows {
                for column in columns.clone() {
                    let cell = (rows.end - 1) * self.columns + column;
                    self.unite(cell, cell + self.columns);
                }
            }
        }
        for number in 0..count {
            let root = self.root(u32::try_from(number).expect("regions fit"));
            self.all[number].set = root;
        }
    }

    /// Joins the regions of `a` and `b` when both are open, the lower number the root.
    fn unite(&mut self, a: usize, b: usize) {
        let (Some(first), Some(second)) = (self.region(a), self.region(b)) else {
            return;
        };
        let (first, second) = (self.root(first), self.root(second));
        let (low, high) = (first.min(second), first.max(second));
        self.parents[high as usize] = low;
    }

    /// The number of the region of open `cell`; `None` for a blocked one.
    fn region(&self, cell: usize) -> Option<u32> {
        let label = self.local[cell];
        (label != 0).then(|| self.starts[self.chunk_of(cell)] + u32::from(label) - 1)
    }

    /// The root of `number`'s joined regions, halving the way there.
    fn root(&mut self, mut number: u32) -> u32 {
        while self.parents[number as usize] != number {
            let parent = self.parents[number as usize];
            self.parents[number as usize] = self.parents[parent as usize];
            number = parent;
        }
        number
    }
}

impl Reach {
    fn add(&mut self, set: u32) {
        let count = usize::from(self.count);
        if self.sets[..count].contains(&set) {
            return;
        }
        self.sets[count] = set;
        self.count += 1;
        self.sets[..=count].sort_unstable();
    }

    pub(crate) const fn is_empty(self) -> bool {
        self.count == 0
    }

    fn contains(self, set: u32) -> bool {
        self.sets[..usize::from(self.count)].contains(&set)
    }

    /// Whether the two share a reachable set: a walker can go from a cell of one to a cell of the
    /// other.
    pub(crate) fn meets(self, other: Reach) -> bool {
        self.sets[..usize::from(self.count)]
            .iter()
            .any(|&set| other.contains(set))
    }
}

const fn widen(value: u32) -> usize {
    value as usize
}

#[cfg(test)]
mod tests {
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
            let quarter = |value: usize| {
                Num::from_bits(i64::try_from(value).unwrap() << (Num::FRAC_BITS - 2))
            };
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
        assert_eq!((&regions.local, &regions.all), (&fresh.local, &fresh.all));
        check(&grid, &blocked, &regions);

        // A cell toggled in chunk 0 while only chunk 1 is marked keeps chunk 0's old labels.
        let cell = 10 * 150 + 10;
        let before = regions.local[cell];
        blocked[cell / 64] ^= 1 << (cell % 64);
        let mut dirty = vec![false; regions.chunks()];
        dirty[1] = true;
        regions.rebuild(&blocked, &dirty);
        assert_eq!(regions.local[cell], before);
    }
}
