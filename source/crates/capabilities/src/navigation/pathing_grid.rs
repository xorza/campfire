use std::ops::Range;

use bevy_ecs::resource::Resource;
use campfire_math::Num;

use crate::navigation::static_index::StaticIndex;
use crate::values::grid::Grid;

/// The map's pathing grid: its bounds in square cells, and for each body radius the mode's
/// walkers have, the cells a walker of that radius cannot stand in, whose centers come closer to
/// a static body, of a living unit that cannot walk, than the two radii together. Derived from
/// the static bodies, not state: a change of them marks again only the cells of the bodies it
/// touched.
#[derive(Resource, Debug)]
pub(crate) struct PathingGrid {
    grid: Grid,
    /// The walkers' radii, ascending, each once: one layer each.
    radii: Vec<Num>,
    /// Words of blocked cells a layer.
    words: usize,
    /// Each layer's blocked cells, one bit a cell, layer after layer.
    blocked: Vec<u64>,
    /// The runs of cells a body taken away blocked, row by row, kept between updates.
    cleared: Vec<Range<usize>>,
}

impl PathingGrid {
    /// The grid over `grid`'s cells for walkers of `radii`, with no static body yet.
    pub(crate) fn new(grid: Grid, mut radii: Vec<Num>) -> PathingGrid {
        radii.sort_unstable();
        radii.dedup();
        let words = grid.cells().div_ceil(64);
        PathingGrid {
            grid,
            blocked: vec![0; words * radii.len()],
            radii,
            words,
            cleared: Vec::new(),
        }
    }

    /// Follows the last update of `index`, which held the static bodies this grid was marked
    /// from: the cells of each body it took away open, then the bodies still near mark them again,
    /// and each body it put in marks its own.
    pub(crate) fn update(&mut self, index: &StaticIndex) {
        let PathingGrid {
            grid,
            radii,
            words,
            blocked,
            cleared,
        } = self;
        for (layer, &radius) in radii.iter().enumerate() {
            let words = &mut blocked[layer * *words..(layer + 1) * *words];
            for body in index.removed() {
                cleared.clear();
                grid.spans_closer(body.at, radius + body.radius, |cells| {
                    for cell in cells.clone() {
                        words[cell / 64] &= !(1 << (cell % 64));
                    }
                    cleared.push(cells);
                });
                // A cell another body blocks too lies within both reaches, so the bodies' centers
                // are closer than the two reaches together.
                let reach = body.radius + radius + radius;
                index.near(body.at.get(), reach, |other| {
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
            for body in index.added() {
                grid.spans_closer(body.at, radius + body.radius, |cells| {
                    for cell in cells {
                        words[cell / 64] |= 1 << (cell % 64);
                    }
                });
            }
        }
    }

    pub(crate) const fn cells(&self) -> usize {
        self.grid.cells()
    }

    /// The cells a walker of `radius`, one of the mode's walkers' radii, may stand in.
    pub(crate) fn layer(&self, radius: Num) -> Layer<'_> {
        let layer = self
            .radii
            .binary_search(&radius)
            .expect("a radius of the mode's walkers");
        Layer {
            grid: &self.grid,
            words: &self.blocked[layer * self.words..(layer + 1) * self.words],
        }
    }
}

/// The cells of the pathing grid a walker of one radius may stand in.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Layer<'a> {
    grid: &'a Grid,
    words: &'a [u64],
}

impl Layer<'_> {
    pub(crate) const fn grid(&self) -> &Grid {
        self.grid
    }

    /// Whether a walker may stand in `cell`.
    pub(crate) const fn open(&self, cell: usize) -> bool {
        self.words[cell / 64] & 1 << (cell % 64) == 0
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use campfire_math::Num;

    use crate::navigation::pathing_grid::PathingGrid;
    use crate::values::bounds::Bounds;
    use crate::values::grid::Grid;

    impl PathingGrid {
        /// A grid of 1 m cells from the origin, for walkers of `radius`, blocked where `rows`,
        /// from z = 0, have `#`.
        pub(crate) fn from_picture(radius: Num, rows: &[&str]) -> PathingGrid {
            let size = |count: usize| Num::from_int(i64::try_from(count).unwrap()).unwrap();
            let bounds = Bounds::new([Num::ZERO; 2], [size(rows[0].len()), size(rows.len())]);
            let mut grid =
                PathingGrid::new(Grid::new(Num::ONE, bounds.unwrap()).unwrap(), vec![radius]);
            for (row, line) in rows.iter().enumerate() {
                for (column, mark) in line.chars().enumerate() {
                    if mark == '#' {
                        let cell = row * line.len() + column;
                        grid.blocked[cell / 64] |= 1 << (cell % 64);
                    }
                }
            }
            grid
        }
    }
}

#[cfg(test)]
mod tests {
    use campfire_math::Vec3;
    use campfire_sim::{IdAllocator, Position};

    use super::*;
    use crate::navigation::static_index::StaticBody;
    use crate::values::bounds::Bounds;

    fn num(value: i64) -> Num {
        Num::from_int(value).unwrap()
    }

    fn half() -> Num {
        Num::from_bits(1 << 23)
    }

    /// Each cell of the 6 × 6 grid of 1 m cells over (−3, −3) to (3, 3), row by row from z = −3,
    /// as `#` where a walker of `radius` cannot stand, `.` where it can.
    fn drawn(grid: &PathingGrid, radius: Num) -> Vec<String> {
        (0..6)
            .map(|row| {
                (0..6)
                    .map(|column| {
                        if grid.layer(radius).open(row * 6 + column) {
                            '.'
                        } else {
                            '#'
                        }
                    })
                    .collect()
            })
            .collect()
    }

    /// A grid over the 6 × 6 cells for walkers of 1 m and 0.5 m.
    fn grid() -> PathingGrid {
        let bounds = Bounds::new([num(-3), num(-3)], [num(3), num(3)]).unwrap();
        PathingGrid::new(
            Grid::new(num(1), bounds).unwrap(),
            vec![Num::ONE, half(), half()],
        )
    }

    /// Marks `grid` for `statics`, which `index` held the bodies before, and checks it against a
    /// grid marked for them alone.
    fn follow(grid: &mut PathingGrid, index: &mut StaticIndex, statics: &[StaticBody]) {
        index.update(statics);
        grid.update(index);
        let (mut alone, mut fresh) = (StaticIndex::new(Num::ONE), self::grid());
        alone.update(statics);
        fresh.update(&alone);
        assert_eq!(grid.blocked, fresh.blocked, "{statics:?}");
    }

    #[test]
    fn a_static_body_blocks_the_cells_closer_than_the_two_radii() {
        let mut grid = grid();
        let mut index = StaticIndex::new(Num::ONE);
        let mut ids = IdAllocator::default();
        let at = |x: Num, z: Num| Position::new(Vec3::new(x, Num::ZERO, z)).unwrap();
        // A tower of 1 m at the origin. Centers sit at ±0.5, ±1.5 and ±2.5. For a walker of 0.5
        // m it blocks the centers closer than 1.5 m: the four at √0.5 ≈ 0.71 m; the next, such as
        // (1.5, 0.5) at √2.5 ≈ 1.58 m, are open. For a walker of 1 m, closer than 2 m: those
        // eight too, but not the corners (1.5, 1.5) at √4.5 ≈ 2.12 m.
        let tower = StaticBody {
            id: ids.allocate(),
            at: at(Num::ZERO, Num::ZERO),
            radius: Num::ONE,
        };
        follow(&mut grid, &mut index, &[tower]);
        let small = ["......", "......", "..##..", "..##..", "......", "......"];
        let large = ["......", "..##..", ".####.", ".####.", "..##..", "......"];
        assert_eq!(drawn(&grid, half()), small);
        assert_eq!(drawn(&grid, Num::ONE), large);

        // A post of 0.5 m at (2, −1.5), and one at (−2.5, 2.5), for a walker of 0.5 m, closer
        // than 1 m: the post blocks the centers (1.5, −1.5) and (2.5, −1.5), 0.5 m off, and not
        // (1.5, −0.5), √1.25 ≈ 1.12 m off. The corner post blocks its own cell; (−1.5, 2.5) is
        // exactly 1 m off, on the edge, and open.
        let post = StaticBody {
            id: ids.allocate(),
            at: at(num(2), -(num(1) + half())),
            radius: half(),
        };
        let corner = StaticBody {
            id: ids.allocate(),
            at: at(-(num(2) + half()), num(2) + half()),
            radius: half(),
        };
        follow(&mut grid, &mut index, &[tower, post, corner]);
        let small = ["......", "....##", "..##..", "..##..", "......", "#....."];
        assert_eq!(drawn(&grid, half()), small);
        // For a walker of 1 m, closer than 1.5 m to the post: its own two cells, the two above
        // and the two below at √1.25 ≈ 1.12 m, such as (1.5, −0.5), which the tower blocks too;
        // (0.5, −1.5), 1.5 m off, is open. The corner post blocks (−2.5, 1.5) and (−1.5, 1.5) at
        // most √2 ≈ 1.41 m off, and (−1.5, 2.5) at 1 m.
        let large = ["....##", "..####", ".#####", ".####.", "####..", "##...."];
        assert_eq!(drawn(&grid, Num::ONE), large);

        // Without the tower, only the others block, and the post still blocks the cells it shares
        // with the tower.
        follow(&mut grid, &mut index, &[post, corner]);
        let small = ["......", "....##", "......", "......", "......", "#....."];
        assert_eq!(drawn(&grid, half()), small);
        let large = ["....##", "....##", "....##", "......", "##....", "##...."];
        assert_eq!(drawn(&grid, Num::ONE), large);

        // The post moves a meter along z, to (2, −0.5).
        let moved = StaticBody {
            at: at(num(2), -half()),
            ..post
        };
        follow(&mut grid, &mut index, &[tower, moved, corner]);
        follow(&mut grid, &mut index, &[]);
        assert_eq!(drawn(&grid, Num::ONE), ["......"; 6]);
    }
}
