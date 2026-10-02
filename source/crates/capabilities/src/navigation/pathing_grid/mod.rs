use std::ops::Range;

use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_math::Num;

use crate::navigation::body_index::{BodyIndex, IndexedBody};
use crate::navigation::regions::Regions;
use crate::navigation::walker::Walker;
use crate::units::body::Body;
use crate::values::grid::Grid;

/// The map's pathing grid: its bounds in square cells, and for each kind of walker the mode has,
/// its clearance: the cells it cannot stand in, whose centers come closer to a static body of its
/// layer, of a living unit that cannot walk, than the two radii together. Derived from the static
/// bodies, not state: a change of them marks again only the cells of the bodies it touched.
#[derive(Resource, Debug)]
pub(crate) struct PathingGrid {
    grid: Grid,
    /// The kinds of walker, in order, each once: one clearance each.
    walkers: Vec<Walker>,
    /// Words of blocked cells a clearance.
    words: usize,
    /// Each clearance's blocked cells, one bit a cell, clearance after clearance.
    blocked: Vec<u64>,
    /// Each clearance's regions.
    regions: Vec<Regions>,
    /// The runs of cells a body taken away blocked, row by row, and the chunks an update touched,
    /// kept between updates.
    cleared: Vec<Range<usize>>,
    dirty: Vec<bool>,
}

impl PathingGrid {
    /// The grid over `grid`'s cells for `walkers`, with no static body yet.
    pub(crate) fn new(grid: Grid, mut walkers: Vec<Walker>) -> PathingGrid {
        walkers.sort_unstable();
        walkers.dedup();
        let words = grid.cells().div_ceil(64);
        let blocked = vec![0; words * walkers.len()];
        let regions: Vec<Regions> = (0..walkers.len())
            .map(|at| Regions::new(&grid, &blocked[at * words..(at + 1) * words]))
            .collect();
        let chunks = regions.first().map_or(0, Regions::chunks);
        PathingGrid {
            grid,
            blocked,
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
            words,
            blocked,
            regions,
            cleared,
            dirty,
        } = self;
        for (at, &Walker { layer, radius }) in walkers.iter().enumerate() {
            let words = &mut blocked[at * *words..(at + 1) * *words];
            let regions = &mut regions[at];
            dirty.fill(false);
            let ours = |body: &&IndexedBody| body.layer == layer;
            for body in index.removed().iter().filter(ours) {
                cleared.clear();
                grid.spans_closer(body.at, radius + body.radius, |cells| {
                    regions.touch(cells.clone(), dirty);
                    for cell in cells.clone() {
                        words[cell / 64] &= !(1 << (cell % 64));
                    }
                    cleared.push(cells);
                });
                // A cell another body blocks too lies within both reaches, so the bodies' centers
                // are closer than the two reaches together.
                let reach = body.radius + radius + radius;
                index.near(layer, body.at.get(), reach, |other| {
                    grid.spans_closer(other.at, radius + other.radius, |cells| {
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
                grid.spans_closer(body.at, radius + body.radius, |cells| {
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
            let walker = Walker::of(world.get::<Body>(entity));
            grid.walkers.binary_search(&walker).is_ok()
        })
    }

    /// The cells `walker`, one of the mode's kinds of walker, may stand in.
    pub(crate) fn clearance(&self, walker: Walker) -> Clearance<'_> {
        let at = self
            .walkers
            .binary_search(&walker)
            .expect("a kind of walker the mode has");
        Clearance {
            grid: &self.grid,
            walker,
            regions: &self.regions[at],
            words: &self.blocked[at * self.words..(at + 1) * self.words],
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

    /// Whether a walker may stand in `cell`.
    pub(crate) const fn open(&self, cell: usize) -> bool {
        self.words[cell / 64] & 1 << (cell % 64) == 0
    }
}

#[cfg(test)]
mod tests;
