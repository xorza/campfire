use bevy_ecs::resource::Resource;
use campfire_math::Num;
use campfire_sim::{Position, StableId};

use crate::values::grid::Grid;

/// The map's pathing grid: its bounds in square cells, and for each body radius the mode's
/// walkers have, the cells a walker of that radius cannot stand in, whose centers come closer to
/// a static body, of a living unit that cannot walk, than the two radii together. Derived from
/// the static bodies, not state: it is built again when they change.
#[derive(Resource, Debug)]
pub(crate) struct PathingGrid {
    grid: Grid,
    /// The walkers' radii, ascending, each once: one layer each.
    radii: Vec<Num>,
    /// Words of blocked cells a layer.
    words: usize,
    /// Each layer's blocked cells, one bit a cell, layer after layer.
    blocked: Vec<u64>,
    /// The static bodies the layers were built from, by stable id.
    built_from: Vec<StaticBody>,
}

/// A living unit that cannot walk, as the pathing grid sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StaticBody {
    pub(crate) id: StableId,
    pub(crate) at: Position,
    pub(crate) radius: Num,
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
            built_from: Vec::new(),
        }
    }

    /// Builds the layers again from `statics`, sorted by stable id, when they are not the static
    /// bodies the layers stand for.
    pub(crate) fn update(&mut self, statics: &[StaticBody]) {
        debug_assert!(statics.is_sorted_by_key(|body| body.id));
        if self.built_from == statics {
            return;
        }
        self.built_from.clear();
        self.built_from.extend_from_slice(statics);
        self.blocked.fill(0);
        for (layer, &radius) in self.radii.iter().enumerate() {
            let words = &mut self.blocked[layer * self.words..(layer + 1) * self.words];
            for body in statics {
                self.grid
                    .spans_closer(body.at, radius + body.radius, |cells| {
                        for cell in cells {
                            words[cell / 64] |= 1 << (cell % 64);
                        }
                    });
            }
        }
    }

    /// Whether a walker of `radius`, one of the mode's walkers' radii, may stand in `cell`.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the long route, the plan's next step, reads the grid"
        )
    )]
    pub(crate) fn open(&self, radius: Num, cell: usize) -> bool {
        let layer = self
            .radii
            .binary_search(&radius)
            .expect("a radius of the mode's walkers");
        self.blocked[layer * self.words + cell / 64] & 1 << (cell % 64) == 0
    }
}

#[cfg(test)]
mod tests {
    use campfire_math::Vec3;
    use campfire_sim::IdAllocator;

    use super::*;
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
                        if grid.open(radius, row * 6 + column) {
                            '.'
                        } else {
                            '#'
                        }
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn a_static_body_blocks_the_cells_closer_than_the_two_radii() {
        let bounds = Bounds::new([num(-3), num(-3)], [num(3), num(3)]).unwrap();
        let mut grid = PathingGrid::new(
            Grid::new(num(1), bounds).unwrap(),
            vec![Num::ONE, half(), half()],
        );
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
        grid.update(&[tower]);
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
        grid.update(&[tower, post, corner]);
        let small = ["......", "....##", "..##..", "..##..", "......", "#....."];
        assert_eq!(drawn(&grid, half()), small);

        // Without the tower, only the others block.
        grid.update(&[post, corner]);
        let small = ["......", "....##", "......", "......", "......", "#....."];
        assert_eq!(drawn(&grid, half()), small);
    }
}
