use std::ops::Range;

use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_math::Num;

use crate::geometry::grid::Grid;
use crate::navigation::body_index::{BodyIndex, IndexedBody};
use crate::navigation::regions::Regions;
use crate::navigation::segment::Segment;
use crate::navigation::terrain::Terrain;
use crate::navigation::walker::Walker;
use crate::units::body::Body;

/// The map's pathing grid: its bounds in square cells, and for each kind of walker the mode has,
/// its clearance: the cells it cannot stand in, those the walls of its layer block and those whose
/// centers come closer than its radius to one of them, and those whose centers come closer to a
/// static body of its layer, of a living unit that cannot walk, than the two radii together, or
/// than its radius to a box.
/// Derived from the map and the static bodies, not state: a change of the bodies marks again only
/// the cells of the bodies it touched.
#[derive(Resource, Debug)]
pub(crate) struct PathingGrid {
    grid: Grid,
    /// The kinds of walker, in order, each once: one clearance each.
    walkers: Vec<Walker>,
    /// Words of blocked cells a clearance.
    words: usize,
    /// Each clearance's blocked cells, one bit a cell, clearance after clearance.
    blocked: Vec<u64>,
    /// The cells of each clearance the walls block, which no body opens, in the same order; the
    /// same cells column by column, so a segment's run of rows in a column is one or two words;
    /// and whether the walls block any.
    walled: Vec<u64>,
    walled_columns: Vec<u64>,
    walls: Vec<bool>,
    /// Each clearance's regions.
    regions: Vec<Regions>,
    /// The runs of cells a body taken away blocked, row by row, and the chunks an update touched,
    /// kept between updates.
    cleared: Vec<Range<usize>>,
    dirty: Vec<bool>,
}

impl PathingGrid {
    /// The grid over `grid`'s cells for `walkers`, with the cells `terrain`'s walls block and no
    /// static body yet.
    pub(crate) fn new(grid: Grid, mut walkers: Vec<Walker>, terrain: &Terrain) -> PathingGrid {
        walkers.sort_unstable();
        walkers.dedup();
        let words = grid.cells().div_ceil(64);
        let mut walled = vec![0; words * walkers.len()];
        let mut walls = vec![false; walkers.len()];
        for (at, walker) in walkers.iter().enumerate() {
            if let Some(blocked) = terrain.blocked(walker.layer) {
                let cells = &mut walled[at * words..(at + 1) * words];
                PathingGrid::clear_of(&grid, blocked, walker.radius, cells);
                walls[at] = true;
            }
        }
        let mut walled_columns = vec![0; walled.len()];
        for at in 0..walkers.len() {
            let span = at * words..(at + 1) * words;
            PathingGrid::by_column(&grid, &walled[span.clone()], &mut walled_columns[span]);
        }
        let blocked = walled.clone();
        let regions: Vec<Regions> = (0..walkers.len())
            .map(|at| Regions::new(&grid, &blocked[at * words..(at + 1) * words]))
            .collect();
        let chunks = regions.first().map_or(0, Regions::chunks);
        PathingGrid {
            grid,
            blocked,
            walled,
            walled_columns,
            walls,
            walkers,
            words,
            regions,
            cleared: Vec::new(),
            dirty: vec![false; chunks],
        }
    }

    /// Follows the last update of `index`, which held the static bodies this grid was marked
    /// from: in each clearance, the cells of each body of its layer that the update took away
    /// open, then the bodies still near mark them again, and each body it put in marks its own;
    /// each clearance's regions are labeled again in the chunks those cells lie in.
    pub(crate) fn update(&mut self, index: &BodyIndex) {
        let PathingGrid {
            grid,
            walkers,
            words: count,
            blocked,
            walled,
            regions,
            cleared,
            dirty,
            ..
        } = self;
        for (at, &Walker { layer, radius }) in walkers.iter().enumerate() {
            let span = at * *count..(at + 1) * *count;
            let (words, walled) = (&mut blocked[span.clone()], &walled[span]);
            let regions = &mut regions[at];
            dirty.fill(false);
            let ours = |body: &&IndexedBody| body.layer == layer;
            for body in index.removed().iter().filter(ours) {
                cleared.clear();
                body.spans_closer(grid, radius, |cells| {
                    regions.touch(cells.clone(), dirty);
                    for cell in cells.clone() {
                        let bit = 1 << (cell % 64);
                        words[cell / 64] = words[cell / 64] & !bit | walled[cell / 64] & bit;
                    }
                    cleared.push(cells);
                });
                // A cell another body blocks too lies within the walker's radius of each, so the
                // other's bounding box comes within this one's bound and twice the radius.
                let reach = body.shape.bound() + radius + radius;
                index.near(layer, body.at.get(), reach, |other| {
                    other.spans_closer(grid, radius, |cells| {
                        let at = cleared.partition_point(|run| run.end <= cells.start);
                        let Some(run) = cleared.get(at) else {
                            return;
                        };
                        for cell in cells.start.max(run.start)..cells.end.min(run.end) {
                            words[cell / 64] |= 1 << (cell % 64);
                        }
                    });
                });
            }
            for body in index.added().iter().filter(ours) {
                body.spans_closer(grid, radius, |cells| {
                    regions.touch(cells.clone(), dirty);
                    for cell in cells {
                        words[cell / 64] |= 1 << (cell % 64);
                    }
                });
            }
            if dirty.contains(&true) {
                regions.rebuild(words, dirty);
            }
        }
    }

    /// The side of a cell.
    pub(crate) const fn cell(&self) -> Num {
        self.grid.cell()
    }

    /// Whether the unit of `entity` in `world` walks as one of the mode's kinds of walker, which
    /// routes and steering read its cells by; every unit does in a match with no pathing grid.
    pub(crate) fn serves(world: &World, entity: Entity) -> bool {
        world.get_resource::<PathingGrid>().is_none_or(|grid| {
            Walker::of(world.get::<Body>(entity))
                .is_some_and(|walker| grid.walkers.binary_search(&walker).is_ok())
        })
    }

    /// The cells `walker` may stand in, when it is one of the mode's kinds of walker.
    pub(crate) fn serving(&self, walker: Walker) -> Option<Clearance<'_>> {
        self.walkers
            .binary_search(&walker)
            .is_ok()
            .then(|| self.clearance(walker))
    }

    /// The cells `walker`, one of the mode's kinds of walker, may stand in.
    pub(crate) fn clearance(&self, walker: Walker) -> Clearance<'_> {
        let at = self
            .walkers
            .binary_search(&walker)
            .expect("a kind of walker the mode has");
        let span = at * self.words..(at + 1) * self.words;
        Clearance {
            grid: &self.grid,
            walker,
            regions: &self.regions[at],
            words: &self.blocked[span.clone()],
            walled: self.walls[at].then(|| &self.walled[span.clone()]),
            walled_columns: self.walls[at].then(|| &self.walled_columns[span]),
        }
    }

    /// Writes into `columns` the cells of `cells`, which number them row by row, column by
    /// column: the cell at `column` and `row` at `column × rows + row`.
    fn by_column(grid: &Grid, cells: &[u64], columns: &mut [u64]) {
        let (width, rows) = (grid.columns(), grid.rows());
        for cell in (0..grid.cells()).filter(|&cell| cells[cell / 64] & 1 << (cell % 64) != 0) {
            let at = cell % width * rows + cell / width;
            columns[at / 64] |= 1 << (at % 64);
        }
    }

    /// Marks in `cells` the clearance of `blocked` for a walker of `radius` on `grid`: each blocked
    /// cell, and each cell whose center comes closer than `radius` to a blocked cell's square,
    /// exactly. A center `i` columns and `j` rows from a cell lies `(2|i| − 1)` half cells from its
    /// square along x, or none at `i` = 0, and the same along z.
    fn clear_of(grid: &Grid, blocked: &[u64], radius: Num, cells: &mut [u64]) {
        let half = i128::from(grid.cell().to_bits());
        let reach = (2 * i128::from(radius.to_bits())).pow(2);
        let off = |index: usize| {
            let index = i128::try_from(index).expect("a cell of the grid");
            (half * (2 * index - 1).max(0)).pow(2)
        };
        // The half width of the stencil's row `j` rows off, for each `j` it reaches.
        let mut widths: Vec<usize> = Vec::new();
        while off(widths.len()) < reach {
            let row = off(widths.len());
            let mut width = 0;
            while row + off(width + 1) < reach {
                width += 1;
            }
            widths.push(width);
        }
        let (columns, rows) = (grid.columns(), grid.rows());
        for cell in (0..grid.cells()).filter(|&cell| blocked[cell / 64] & 1 << (cell % 64) != 0) {
            cells[cell / 64] |= 1 << (cell % 64);
            let (column, row) = (cell % columns, cell / columns);
            for (off, &width) in widths.iter().enumerate() {
                let low = column.saturating_sub(width);
                let high = (column + width).min(columns - 1);
                for near in [row.checked_sub(off), Some(row + off).filter(|&z| z < rows)] {
                    for near in near
                        .into_iter()
                        .flat_map(|z| (low..=high).map(move |x| z * columns + x))
                    {
                        cells[near / 64] |= 1 << (near % 64);
                    }
                }
            }
        }
    }
}

/// The cells of the pathing grid one kind of walker may stand in.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Clearance<'a> {
    grid: &'a Grid,
    walker: Walker,
    regions: &'a Regions,
    words: &'a [u64],
    /// The cells the walls block it from, when they block any, row by row and column by column.
    walled: Option<&'a [u64]>,
    walled_columns: Option<&'a [u64]>,
}

impl Clearance<'_> {
    pub(crate) const fn grid(&self) -> &Grid {
        self.grid
    }

    pub(crate) const fn walker(&self) -> Walker {
        self.walker
    }

    pub(crate) const fn regions(&self) -> &Regions {
        self.regions
    }

    /// Whether `segment` touches a cell the walls block the walker from, exactly, corners
    /// included; each cell it visits to tell adds to `work`.
    pub(crate) fn walled(&self, segment: Segment, work: &mut u32) -> bool {
        self.walled_columns.is_some_and(|walled| {
            let rows = self.grid.rows();
            let (from, to) = (segment.start(), segment.end());
            self.grid.touches_columns(from, to, |column, run| {
                let cells = column * rows + run.start..column * rows + run.end;
                let hit = Clearance::first_set(walled, cells.clone());
                let visited = hit.map_or(cells.end, |at| at + 1) - cells.start;
                *work += u32::try_from(visited).expect("a column's rows fit u32");
                hit.is_some()
            })
        })
    }

    /// The first of `cells` set in `words`.
    fn first_set(words: &[u64], cells: Range<usize>) -> Option<usize> {
        let mut at = cells.start;
        while at < cells.end {
            let shift = at % 64;
            let mut bits = words[at / 64] >> shift;
            let left = cells.end - at;
            if left < 64 - shift {
                bits &= (1 << left) - 1;
            }
            if bits != 0 {
                return Some(at + bits.trailing_zeros() as usize);
            }
            at += 64 - shift;
        }
        None
    }

    /// Whether the step `segment` touches a cell the walls block the walker from, exactly, but the
    /// cell it starts in: a walker collision pushed into a wall's margin may still step out.
    pub(crate) fn walls_block_step(&self, segment: Segment) -> bool {
        self.walled.is_some_and(|walled| {
            let start = self.grid.nearest_cell(segment.start());
            let blocked = |cell: usize| cell != start && walled[cell / 64] & 1 << (cell % 64) != 0;
            self.grid.touches(segment.start(), segment.end(), blocked)
        })
    }

    /// Whether a walker may stand in `cell`.
    pub(crate) const fn open(&self, cell: usize) -> bool {
        self.words[cell / 64] & 1 << (cell % 64) == 0
    }
}

#[cfg(test)]
mod tests;
