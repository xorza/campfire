use std::mem;
use std::ops::Range;

use campfire_sim::Position;

use crate::values::grid::Grid;

/// The cells of one layer of the pathing grid a walker can go between, as 0 A.D.'s hierarchical
/// pathfinder keeps them: the grid in chunks of `CHUNK` × `CHUNK` cells, each chunk's open cells
/// split into regions of cells that touch along a side, regions of chunks side by side joined where
/// their cells touch, and the joined regions numbered as reachable sets. Touching along a side is
/// exact for the moves the planner takes: a diagonal step needs both cells beside it open. Each
/// chunk keeps the pairs of its regions and its neighbors' whose cells touch across its sides, so
/// a build labels again only the chunks it is told changed, finds again only the pairs along
/// their sides, and then numbers the reachable sets over the regions and their pairs, not the
/// cells, as 0 A.D. does. Derived from the layer, not state.
#[derive(Debug)]
pub(crate) struct Regions {
    columns: usize,
    rows: usize,
    chunk_columns: usize,
    /// Each cell's region in its chunk, counted from 1; 0 for a blocked cell.
    local: Vec<u16>,
    /// Each chunk's regions and pairs. A chunk's are made again alone, so each has its own.
    chunks: Vec<Chunk>,
    /// The number of each chunk's first region among all, chunk after chunk, and past the last.
    starts: Vec<u32>,
    /// A chunk's runs of open cells as it is labeled, each run's parent as runs join and its
    /// label, and each region's parent as regions join.
    runs: Vec<Run>,
    run_parents: Vec<u32>,
    run_labels: Vec<u16>,
    parents: Vec<u32>,
}

/// A run of a chunk's open cells along one row: its row, and its columns from `start` to before
/// `end`, counted from the chunk's first.
#[derive(Debug, Clone, Copy)]
struct Run {
    row: u32,
    start: u32,
    end: u32,
}

/// One chunk's regions, by label, and the pairs of regions that touch across its right and lower
/// sides, sorted, each once.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Chunk {
    regions: Vec<Region>,
    pairs: Vec<Pair>,
}

/// A region of a chunk: the box of its cells, columns and rows from `low` to `high` both in, and
/// its reachable set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Region {
    low: [u32; 2],
    high: [u32; 2],
    set: u32,
}

/// Two regions whose cells touch across a side of a chunk: one of the chunk and one of the
/// neighbor on that side, by label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Pair {
    side: Side,
    here: u16,
    there: u16,
}

/// The side of a chunk a pair touches across.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Side {
    Right,
    Lower,
}

/// The reachable sets a cell belongs to: its own region's, or for a blocked cell, which a walker
/// may leave or enter, those of the open cells beside it; the first `count`, ascending.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Reach {
    sets: [u32; 4],
    count: u8,
}

/// A region the nearest cell to a goal may lie in: by how near its box comes to the goal, in
/// squares of half bits, then by its chunk and label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Candidate {
    bound: u128,
    chunk: u32,
    label: u16,
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
            chunks: vec![Chunk::default(); chunks],
            starts: Vec::with_capacity(chunks + 1),
            runs: Vec::new(),
            run_parents: Vec::new(),
            run_labels: Vec::new(),
            parents: Vec::new(),
        };
        regions.build(blocked, |_| true);
        regions
    }

    /// Labels again the chunks `dirty` marks, from `blocked`, finds again the pairs along their
    /// sides, then numbers the reachable sets again.
    pub(crate) fn rebuild(&mut self, blocked: &[u64], dirty: &[bool]) {
        debug_assert_eq!(dirty.len(), self.chunks.len());
        self.build(blocked, |chunk| dirty[chunk]);
    }

    /// The number of chunks, for a caller that marks those a change touched.
    pub(crate) const fn chunks(&self) -> usize {
        self.chunks.len()
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
        for (chunk, held) in (0..).zip(&self.chunks) {
            let regions = (1..).zip(&held.regions);
            candidates.extend(
                regions
                    .filter(|(_, region)| reach.contains(region.set))
                    .map(|(label, region)| Candidate {
                        bound: grid.box_distance(
                            region.low.map(widen),
                            region.high.map(widen),
                            goal,
                        ),
                        chunk,
                        label,
                    }),
            );
        }
        candidates.sort_unstable();
        *work += *self.starts.last().expect("a start past the last chunk");
        let mut best: Option<(u128, usize)> = None;
        for candidate in &*candidates {
            if best.is_some_and(|(distance, _)| candidate.bound > distance) {
                break;
            }
            let label = candidate.label;
            let region = self.chunks[candidate.chunk as usize].regions[usize::from(label) - 1];
            let [columns, rows] = [0, 1].map(|axis| region.high[axis] - region.low[axis] + 1);
            *work += columns * rows;
            for row in widen(region.low[1])..=widen(region.high[1]) {
                for column in widen(region.low[0])..=widen(region.high[0]) {
                    let cell = row * self.columns + column;
                    if self.local[cell] != label {
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
        let chunk = &self.chunks[self.chunk_of(cell)];
        Some(chunk.regions[usize::from(label) - 1].set)
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

    /// Labels again each chunk `relabel` names, keeps the regions of the others, finds again the
    /// pairs along the sides of those it labeled, and joins them.
    fn build(&mut self, blocked: &[u64], relabel: impl Fn(usize) -> bool) {
        for chunk in 0..self.chunks.len() {
            if relabel(chunk) {
                self.label(chunk, blocked);
            }
        }
        for chunk in 0..self.chunks.len() {
            let right = (chunk + 1) % self.chunk_columns != 0 && relabel(chunk + 1);
            let lower = chunk + self.chunk_columns < self.chunks.len()
                && relabel(chunk + self.chunk_columns);
            if relabel(chunk) || right || lower {
                self.pair(chunk);
            }
        }
        self.join();
    }

    /// Splits the open cells of `chunk` into regions of cells that touch along a side, numbered
    /// in the order of their first cell, row by row, in place of its regions. Each row's open
    /// cells, one word for the chunk's row, split into runs; a run joins each run of the row
    /// before it overlaps, the earlier run the root, so each region's root is its first run,
    /// which holds its first cell.
    fn label(&mut self, chunk: usize, blocked: &[u64]) {
        let [columns, rows] = self.bounds(chunk);
        let width = columns.end - columns.start;
        let full = if width == 64 {
            u64::MAX
        } else {
            (1 << width) - 1
        };
        self.runs.clear();
        self.run_parents.clear();
        let mut before = 0..0;
        for row in rows.clone() {
            let first = row * self.columns + columns.start;
            self.local[first..first + width].fill(0);
            let mut open = !Regions::bits(blocked, first) & full;
            let here = self.runs.len();
            let mut above = before.start;
            while open != 0 {
                let start = open.trailing_zeros();
                let end = start + (!(open >> start)).trailing_zeros();
                open &= u64::MAX.checked_shl(end).unwrap_or(0);
                let at = u32::try_from(self.runs.len()).expect("a chunk's runs fit u32");
                self.runs.push(Run {
                    row: u32::try_from(row).expect("a grid has at most 2²² cells"),
                    start,
                    end,
                });
                self.run_parents.push(at);
                // The runs of the row before that overlap this one, which share a side with it.
                while above < before.end && self.runs[above].end <= start {
                    above += 1;
                }
                let mut over = above;
                while over < before.end && self.runs[over].start < end {
                    let earlier = u32::try_from(over).expect("a chunk's runs fit u32");
                    Regions::unite(&mut self.run_parents, earlier, at);
                    over += 1;
                }
            }
            before = here..self.runs.len();
        }
        let mut regions = mem::take(&mut self.chunks[chunk].regions);
        regions.clear();
        self.run_labels.clear();
        let column = u32::try_from(columns.start).expect("a grid has at most 2²² cells");
        for at in 0..self.runs.len() {
            let run = self.runs[at];
            let number = u32::try_from(at).expect("a chunk's runs fit u32");
            let root = Regions::root(&mut self.run_parents, number) as usize;
            let (low, high) = (
                [column + run.start, run.row],
                [column + run.end - 1, run.row],
            );
            let label = if root == at {
                regions.push(Region { low, high, set: 0 });
                u16::try_from(regions.len()).expect("a chunk has at most 2¹² regions")
            } else {
                let label = self.run_labels[root];
                let region = &mut regions[usize::from(label) - 1];
                region.low = [region.low[0].min(low[0]), region.low[1].min(low[1])];
                region.high = [region.high[0].max(high[0]), region.high[1].max(high[1])];
                label
            };
            self.run_labels.push(label);
            let first = run.row as usize * self.columns + columns.start;
            self.local[first + run.start as usize..first + run.end as usize].fill(label);
        }
        self.chunks[chunk].regions = regions;
    }

    /// The 64 cells' bits of `blocked` from cell `at` on.
    fn bits(blocked: &[u64], at: usize) -> u64 {
        let (word, shift) = (at / 64, at % 64);
        let low = blocked[word] >> shift;
        let high = match blocked.get(word + 1) {
            Some(&next) if shift > 0 => next << (64 - shift),
            _ => 0,
        };
        low | high
    }

    /// Finds the pairs of regions whose cells touch across `chunk`'s right and lower sides, in
    /// place of its pairs.
    fn pair(&mut self, chunk: usize) {
        let [columns, rows] = self.bounds(chunk);
        let mut pairs = mem::take(&mut self.chunks[chunk].pairs);
        pairs.clear();
        let mut touch = |side: Side, cell: usize, beyond: usize| {
            let (here, there) = (self.local[cell], self.local[beyond]);
            if here != 0 && there != 0 {
                pairs.push(Pair { side, here, there });
            }
        };
        if columns.end < self.columns {
            for row in rows.clone() {
                let cell = row * self.columns + columns.end - 1;
                touch(Side::Right, cell, cell + 1);
            }
        }
        if rows.end < self.rows {
            for column in columns {
                let cell = (rows.end - 1) * self.columns + column;
                touch(Side::Lower, cell, cell + self.columns);
            }
        }
        pairs.sort_unstable();
        pairs.dedup();
        self.chunks[chunk].pairs = pairs;
    }

    /// Numbers the regions chunk after chunk, joins each pair, and gives each region the lowest
    /// number of the regions joined to it as its reachable set.
    fn join(&mut self) {
        self.starts.clear();
        let mut count = 0;
        for chunk in &self.chunks {
            self.starts.push(count);
            count += u32::try_from(chunk.regions.len()).expect("regions fit their cells");
        }
        self.starts.push(count);
        self.parents.clear();
        self.parents.extend(0..count);
        for (chunk, held) in self.chunks.iter().enumerate() {
            for pair in &held.pairs {
                let beyond = match pair.side {
                    Side::Right => chunk + 1,
                    Side::Lower => chunk + self.chunk_columns,
                };
                let here = self.starts[chunk] + u32::from(pair.here) - 1;
                let there = self.starts[beyond] + u32::from(pair.there) - 1;
                Regions::unite(&mut self.parents, here, there);
            }
        }
        for (chunk, held) in self.chunks.iter_mut().enumerate() {
            for (number, region) in (self.starts[chunk]..).zip(&mut held.regions) {
                region.set = Regions::root(&mut self.parents, number);
            }
        }
    }

    /// Joins the regions, or runs, numbered `a` and `b` in `parents`, the lower number the root.
    fn unite(parents: &mut [u32], a: u32, b: u32) {
        let (first, second) = (Regions::root(parents, a), Regions::root(parents, b));
        let (low, high) = (first.min(second), first.max(second));
        parents[high as usize] = low;
    }

    /// The root of `number`'s joined regions in `parents`, halving the way there.
    const fn root(parents: &mut [u32], mut number: u32) -> u32 {
        while parents[number as usize] != number {
            let parent = parents[number as usize];
            parents[number as usize] = parents[parent as usize];
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
mod tests;
