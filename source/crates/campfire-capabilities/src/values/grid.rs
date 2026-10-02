use std::ops::Range;

use campfire_math::{Num, Vec3};
use campfire_sim::Position;

use crate::values::bounds::Bounds;

/// A map's ground grid: square cells of `cell` meters over its bounds, whole cells from their min
/// until they cover their max. Cells are numbered along x, then along z.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grid {
    cell: Num,
    bounds: Bounds,
    size: [u32; 2],
}

impl Grid {
    /// The most cells a grid holds.
    const MAX_CELLS: u64 = 1 << 22;

    /// The grid of `cell`-meter cells over `bounds`; `None` unless `cell` is positive and at most
    /// the world's bound, and the cells are at most `MAX_CELLS`.
    pub(crate) fn new(cell: Num, bounds: Bounds) -> Option<Grid> {
        if cell <= Num::ZERO || cell > Position::BOUND {
            return None;
        }
        let (min, max) = (bounds.min(), bounds.max());
        let mut size = [0; 2];
        for axis in 0..2 {
            let span = (max[axis] - min[axis]).to_bits().unsigned_abs();
            let cells = span.div_ceil(cell.to_bits().unsigned_abs());
            size[axis] = u32::try_from(cells).ok()?;
        }
        if u64::from(size[0]) * u64::from(size[1]) > Grid::MAX_CELLS {
            return None;
        }
        Some(Grid { cell, bounds, size })
    }

    pub(crate) const fn cells(&self) -> usize {
        self.size[0] as usize * self.size[1] as usize
    }

    /// The side of a cell.
    pub(crate) const fn cell(&self) -> Num {
        self.cell
    }

    /// The cells in a row, along x.
    pub(crate) const fn columns(&self) -> usize {
        self.size[0] as usize
    }

    pub(crate) const fn rows(&self) -> usize {
        self.size[1] as usize
    }

    /// The point of the bounds nearest `pos` on the ground plane, at its height.
    pub(crate) fn clamp(&self, pos: Position) -> Position {
        self.bounds.clamp(pos)
    }

    /// The cell of the point of the bounds nearest `pos`.
    pub(crate) fn nearest_cell(&self, pos: Position) -> usize {
        self.cell_of(self.clamp(pos))
            .expect("a point of the bounds is in a cell")
    }

    /// The square of the distance from `pos` to the nearest center of the cells from column and
    /// row `low` to `high`, both in, on the ground plane, in halves of a bit, exactly.
    pub(crate) fn box_distance(&self, low: [usize; 2], high: [usize; 2], pos: Position) -> u128 {
        let twice = |value: Num| 2 * i128::from(value.to_bits());
        let step = i128::from(self.cell.to_bits());
        let min = self.bounds.min();
        let index = |index: usize| i128::try_from(index).expect("a cell of the grid");
        let at = [twice(pos.get().x), twice(pos.get().z)];
        let offset = |axis: usize| {
            let center = |index: i128| twice(min[axis]) + step * (2 * index + 1);
            let nearest = at[axis].clamp(center(index(low[axis])), center(index(high[axis])));
            at[axis] - nearest
        };
        let (dx, dz) = (offset(0), offset(1));
        (dx * dx + dz * dz).cast_unsigned()
    }

    /// The square of the distance from the center of `cell` to `pos` on the ground plane, in
    /// halves of a bit, exactly.
    pub(crate) fn center_distance(&self, cell: usize, pos: Position) -> u128 {
        let at = [cell % self.columns(), cell / self.columns()];
        self.box_distance(at, at, pos)
    }

    /// The center of `cell` at height `y`, rounded down to a whole bit, or the point of the bounds
    /// nearest it: a cell of the last row or column may reach past them.
    pub(crate) fn center(&self, cell: usize, y: Num) -> Position {
        let (column, row) = (cell % self.columns(), cell / self.columns());
        let along = |axis: usize, index: usize| {
            let index = i64::try_from(index).expect("a cell of the grid");
            let cell = self.cell.to_bits();
            Num::from_bits(self.bounds.min()[axis].to_bits() + cell * index + cell / 2)
        };
        let [x, z] = self.bounds.clamp_ground([along(0, column), along(1, row)]);
        Position::new(Vec3::new(x, y, z)).expect("bounds are within the world's bound")
    }

    /// The cell `pos` stands in; `None` outside the bounds. A point on the line between two cells
    /// is in the one after it, and a point on the max edge in the last.
    pub(crate) fn cell_of(&self, pos: Position) -> Option<usize> {
        if !self.bounds.contains(pos) {
            return None;
        }
        let at = pos.get();
        let x = self.index(0, at.x);
        let z = self.index(1, at.z);
        Some(z * self.size[0] as usize + x)
    }

    /// Calls `reveal` with each row's run of the cells whose centers are within `radius` of `pos`
    /// on the ground plane, exactly, as every range is; rows in order.
    pub(crate) fn spans_within(
        &self,
        pos: Position,
        radius: Num,
        reveal: impl FnMut(Range<usize>),
    ) {
        self.spans(pos, radius, false, reveal);
    }

    /// Calls `mark` with each row's run of the cells whose centers are closer than `radius` to
    /// `pos` on the ground plane, exactly: a center at `radius` is not closer. Rows in order.
    pub(crate) fn spans_closer(&self, pos: Position, radius: Num, mark: impl FnMut(Range<usize>)) {
        self.spans(pos, radius, true, mark);
    }

    /// The runs of `spans_within`, or of `spans_closer` when `strict`: with whole half-bits, a
    /// square below `reach²` is one at most `reach² − 1`.
    fn spans(
        &self,
        pos: Position,
        radius: Num,
        strict: bool,
        mut reveal: impl FnMut(Range<usize>),
    ) {
        let at = pos.get();
        let cell = self.cell.to_bits();
        // In halves of a bit, so each cell's center, half a cell from its edge, is whole. A
        // position and `min` are within 2⁴⁴ bits of the origin and a center within 2⁴⁶, so every
        // center is within 8 bounds: 2⁴⁸ halves, which leaves i64 room for sums, and i128 for
        // squares.
        let twice = |value: Num| 2 * value.to_bits();
        let reach = 2 * radius.to_bits().min(8 * Position::BOUND.to_bits());
        let size = self.size.map(i64::from);
        let min = self.bounds.min();
        let from = [twice(at.x) - twice(min[0]), twice(at.z) - twice(min[1])];
        let low_row = (from[1] - reach).div_euclid(2 * cell).max(0);
        let high_row = (from[1] + reach).div_euclid(2 * cell).min(size[1] - 1);
        for z in low_row..=high_row {
            let dz = i128::from(from[1] - cell * (2 * z + 1));
            let rest = i128::from(reach).pow(2) - i128::from(strict) - dz * dz;
            if rest < 0 {
                continue;
            }
            let half = i64::try_from(rest.cast_unsigned().isqrt()).expect("a root within reach");
            // A center at 2x + 1 half cells is within `half` of `from[0]`.
            let low_odd = -(half - from[0]).div_euclid(cell);
            let high_odd = (from[0] + half).div_euclid(cell);
            let low = (-(1 - low_odd).div_euclid(2)).max(0);
            let high = (high_odd - 1).div_euclid(2).min(size[0] - 1);
            if low <= high {
                let start = z * size[0];
                let cells = |x: i64| usize::try_from(start + x).expect("a cell of the grid");
                reveal(cells(low)..cells(high) + 1);
            }
        }
    }

    /// The index along `axis` of `at`, which is within the bounds.
    fn index(&self, axis: usize, at: Num) -> usize {
        let offset = at.to_bits() - self.bounds.min()[axis].to_bits();
        let index = (offset / self.cell.to_bits()).min(i64::from(self.size[axis]) - 1);
        usize::try_from(index).expect("a point within the bounds is past their min")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn at(x: Num, z: Num) -> Position {
        Position::new(Vec3::new(x, Num::ZERO, z)).unwrap()
    }

    #[test]
    fn a_grid_covers_its_rectangle_in_whole_cells_and_reveals_exactly() {
        // 1 m cells over (−2, −1) to (2, 1.5): 4 along x, 3 along z, the last row half outside.
        let half = Num::HALF;
        let bounds = Bounds::new(
            [Num::int(-2), Num::int(-1)],
            [Num::int(2), Num::int(1) + half],
        )
        .unwrap();
        let grid = Grid::new(Num::int(1), bounds).unwrap();
        assert_eq!(grid.cells(), 12);
        // Cell (2, 1) is x from 0 to 1, z from 0 to 1: number 1 × 4 + 2 = 6.
        assert_eq!(grid.cell_of(at(Num::ZERO, Num::ZERO)), Some(6));
        assert_eq!(
            grid.cell_of(at(Num::int(1) - Num::EPSILON, Num::int(1) - Num::EPSILON)),
            Some(6)
        );
        assert_eq!(grid.cell_of(at(Num::int(-2), Num::int(-1))), Some(0));
        // On the max edges: x = 2 ends cell column 3, the last, so (2, 0) is in 1 × 4 + 3 = 7;
        // z = 1.5 is inside row 2, so (0, 1.5) is in 2 × 4 + 2 = 10, and the corner in 11.
        let edges = [
            at(Num::int(2), Num::ZERO),
            at(Num::ZERO, Num::int(1) + half),
            at(Num::int(2), Num::int(1) + half),
        ];
        assert_eq!(
            edges.map(|pos| grid.cell_of(pos)),
            [Some(7), Some(10), Some(11)]
        );
        // Outside the bounds, even within the last row's cells, which reach z = 2.
        let off = [
            at(Num::int(2) + Num::EPSILON, Num::ZERO),
            at(Num::int(-2) - Num::EPSILON, Num::ZERO),
            at(Num::ZERO, Num::int(1) + half + Num::EPSILON),
        ];
        assert_eq!(off.map(|pos| grid.cell_of(pos)), [None, None, None]);

        // From (0, 0) the nearest centers, (±0.5, ±0.5), are √0.5 ≈ 0.707 m away, the next
        // ones, such as (1.5, 0.5), √2.5 ≈ 1.58 m. A radius of 1.5 reaches only the four.
        let reveal = |radius: Num| {
            let mut cells = Vec::new();
            grid.spans_within(at(Num::ZERO, Num::ZERO), radius, |span| cells.extend(span));
            cells
        };
        assert_eq!(reveal(Num::int(1) + half), [1, 2, 5, 6]);
        // A radius of 1.59 reaches the centers √2.5 ≈ 1.581 away too: the ring around the four,
        // less the cells off the grid and the corners, √4.5 away.
        let reach = Num::from_bits((159 << 24) / 100);
        assert_eq!(reveal(reach), [0, 1, 2, 3, 4, 5, 6, 7, 9, 10]);
        assert!(reveal(Num::ZERO).is_empty());
        // A radius past every center, the largest number among them, reveals every cell: the
        // reach stops at 8 bounds before it doubles.
        let every: Vec<usize> = (0..grid.cells()).collect();
        assert_eq!(reveal(Num::MAX), every);
        assert_eq!(reveal(Position::BOUND), every);

        // Against each cell's center tested alone, in halves of a bit, from points on and off
        // the grid, on cell lines and between them, with radii that end on centers and between.
        let quarter = Num::QUARTER;
        let twice = |value: Num| 2 * i128::from(value.to_bits());
        for x in -12..12 {
            for z in -8..10 {
                let pos = at(quarter * x, quarter * z);
                for radius in [0, 1, 2, 3, 5, 6, 7, 9, 12, 20].map(|r| quarter * r) {
                    // Within the radius, and strictly closer than it.
                    for strict in [false, true] {
                        let mut spans = Vec::new();
                        if strict {
                            grid.spans_closer(pos, radius, |span| spans.extend(span));
                        } else {
                            grid.spans_within(pos, radius, |span| spans.extend(span));
                        }
                        let alone: Vec<usize> = (0..grid.cells())
                            .filter(|&cell| {
                                let center = |index: usize, axis: usize| {
                                    twice(grid.bounds.min()[axis])
                                        + i128::from(grid.cell.to_bits()) * (2 * index as i128 + 1)
                                };
                                let dx = twice(pos.get().x) - center(cell % 4, 0);
                                let dz = twice(pos.get().z) - center(cell / 4, 1);
                                let square = dx * dx + dz * dz;
                                let reach = twice(radius) * twice(radius);
                                if strict {
                                    square < reach
                                } else {
                                    square <= reach
                                }
                            })
                            .collect();
                        assert_eq!(spans, alone, "{x} {z} {radius:?} {strict}");
                    }
                }
            }
        }

        // 2048 × 2049 cells are more than 2²² = 2048 × 2048.
        let wide =
            Bounds::new([Num::int(0), Num::int(0)], [Num::int(2048), Num::int(2049)]).unwrap();
        let square =
            Bounds::new([Num::int(0), Num::int(0)], [Num::int(2048), Num::int(2048)]).unwrap();
        assert_eq!(
            Grid::new(Num::int(1), square).map(|grid| grid.cells()),
            Some(1 << 22)
        );
        for (cell, bounds) in [
            (Num::ZERO, bounds),
            (Position::BOUND + Num::EPSILON, bounds),
            (Num::int(1), wide),
        ] {
            assert_eq!(Grid::new(cell, bounds), None, "{cell:?} {bounds:?}");
        }
    }
}
