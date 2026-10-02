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

    pub(crate) const fn cells(&self) -> usize {
        self.grid.cells()
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
mod tests {
    use campfire_math::Vec3;
    use campfire_sim::{IdAllocator, Position};

    use super::*;
    use crate::units::layer::Layer;
    use crate::values::bounds::Bounds;

    fn num(value: i64) -> Num {
        Num::from_int(value).unwrap()
    }

    fn half() -> Num {
        Num::from_bits(1 << 23)
    }

    /// A walker of `radius` on the first layer.
    fn ground(radius: Num) -> Walker {
        Walker {
            layer: Layer::FIRST,
            radius,
        }
    }

    /// A walker of 1 m on the second layer.
    fn air() -> Walker {
        Walker {
            layer: Layer::new(1),
            radius: Num::ONE,
        }
    }

    /// Each cell of the 6 × 6 grid of 1 m cells over (−3, −3) to (3, 3), row by row from z = −3,
    /// as `#` where `walker` cannot stand, `.` where it can.
    fn drawn(grid: &PathingGrid, walker: Walker) -> Vec<String> {
        (0..6)
            .map(|row| {
                (0..6)
                    .map(|column| {
                        if grid.clearance(walker).open(row * 6 + column) {
                            '.'
                        } else {
                            '#'
                        }
                    })
                    .collect()
            })
            .collect()
    }

    /// A grid over the 6 × 6 cells for walkers of 1 m and 0.5 m on the ground, and of 1 m in
    /// the air.
    fn grid() -> PathingGrid {
        let bounds = Bounds::new([num(-3), num(-3)], [num(3), num(3)]).unwrap();
        PathingGrid::new(
            Grid::new(num(1), bounds).unwrap(),
            vec![air(), ground(Num::ONE), ground(half()), ground(half())],
        )
    }

    /// Marks `grid` for `statics`, which `index` held the bodies before, and checks it against a
    /// grid marked for them alone.
    fn follow(grid: &mut PathingGrid, index: &mut BodyIndex, statics: &[IndexedBody]) {
        index.update(statics);
        grid.update(index);
        let (mut alone, mut fresh) = (BodyIndex::new(Num::ONE), self::grid());
        alone.update(statics);
        fresh.update(&alone);
        assert_eq!(grid.blocked, fresh.blocked, "{statics:?}");
    }

    #[test]
    fn a_static_body_blocks_the_cells_closer_than_the_two_radii() {
        let mut grid = grid();
        let mut index = BodyIndex::new(Num::ONE);
        let mut ids = IdAllocator::default();
        let at = |x: Num, z: Num| Position::new(Vec3::new(x, Num::ZERO, z)).unwrap();
        // A tower of 1 m at the origin. Centers sit at ±0.5, ±1.5 and ±2.5. For a walker of 0.5
        // m it blocks the centers closer than 1.5 m: the four at √0.5 ≈ 0.71 m; the next, such as
        // (1.5, 0.5) at √2.5 ≈ 1.58 m, are open. For a walker of 1 m, closer than 2 m: those
        // eight too, but not the corners (1.5, 1.5) at √4.5 ≈ 2.12 m.
        let tower = IndexedBody {
            id: ids.allocate(),
            at: at(Num::ZERO, Num::ZERO),
            radius: Num::ONE,
            layer: Layer::FIRST,
        };
        follow(&mut grid, &mut index, &[tower]);
        let small = ["......", "......", "..##..", "..##..", "......", "......"];
        let large = ["......", "..##..", ".####.", ".####.", "..##..", "......"];
        assert_eq!(drawn(&grid, ground(half())), small);
        assert_eq!(drawn(&grid, ground(Num::ONE)), large);

        // A post of 0.5 m at (2, −1.5), and one at (−2.5, 2.5), for a walker of 0.5 m, closer
        // than 1 m: the post blocks the centers (1.5, −1.5) and (2.5, −1.5), 0.5 m off, and not
        // (1.5, −0.5), √1.25 ≈ 1.12 m off. The corner post blocks its own cell; (−1.5, 2.5) is
        // exactly 1 m off, on the edge, and open.
        let post = IndexedBody {
            id: ids.allocate(),
            at: at(num(2), -(num(1) + half())),
            radius: half(),
            layer: Layer::FIRST,
        };
        let corner = IndexedBody {
            id: ids.allocate(),
            at: at(-(num(2) + half()), num(2) + half()),
            radius: half(),
            layer: Layer::FIRST,
        };
        follow(&mut grid, &mut index, &[tower, post, corner]);
        let small = ["......", "....##", "..##..", "..##..", "......", "#....."];
        assert_eq!(drawn(&grid, ground(half())), small);
        // For a walker of 1 m, closer than 1.5 m to the post: its own two cells, the two above
        // and the two below at √1.25 ≈ 1.12 m, such as (1.5, −0.5), which the tower blocks too;
        // (0.5, −1.5), 1.5 m off, is open. The corner post blocks (−2.5, 1.5) and (−1.5, 1.5) at
        // most √2 ≈ 1.41 m off, and (−1.5, 2.5) at 1 m.
        let large = ["....##", "..####", ".#####", ".####.", "####..", "##...."];
        assert_eq!(drawn(&grid, ground(Num::ONE)), large);
        // None of them is in the air; a cloud of 1 m at the origin is, and blocks for the air
        // walker of 1 m what the tower blocks for the ground walker of 1 m, and nothing on the
        // ground.
        assert_eq!(drawn(&grid, air()), ["......"; 6]);
        let cloud = IndexedBody {
            id: ids.allocate(),
            layer: air().layer,
            ..tower
        };
        follow(&mut grid, &mut index, &[tower, post, corner, cloud]);
        assert_eq!(drawn(&grid, ground(half())), small);
        assert_eq!(drawn(&grid, ground(Num::ONE)), large);
        let under = ["......", "..##..", ".####.", ".####.", "..##..", "......"];
        assert_eq!(drawn(&grid, air()), under);

        // Without the tower, only the others block, and the post still blocks the cells it shares
        // with the tower.
        follow(&mut grid, &mut index, &[post, corner]);
        let small = ["......", "....##", "......", "......", "......", "#....."];
        assert_eq!(drawn(&grid, ground(half())), small);
        let large = ["....##", "....##", "....##", "......", "##....", "##...."];
        assert_eq!(drawn(&grid, ground(Num::ONE)), large);

        // The post moves a meter along z, to (2, −0.5).
        let moved = IndexedBody {
            at: at(num(2), -half()),
            ..post
        };
        follow(&mut grid, &mut index, &[tower, moved, corner]);
        assert_eq!(drawn(&grid, air()), ["......"; 6]);
        follow(&mut grid, &mut index, &[]);
        assert_eq!(drawn(&grid, ground(Num::ONE)), ["......"; 6]);
    }
}
