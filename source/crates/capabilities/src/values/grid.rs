use std::ops::Range;

use campfire_math::Num;
use campfire_sim::Position;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::values::scalar::Scalar;

/// A map's ground grid: square cells of `cell` meters, whole cells from `min` on the ground plane
/// until they cover `max`. Cells are numbered along x, then along z.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grid {
    cell: Num,
    min: [Num; 2],
    size: [u32; 2],
}

impl Grid {
    /// The most cells a grid holds.
    const MAX_CELLS: u64 = 1 << 22;

    /// The grid of `cell`-meter cells from `min` to cover `max`, each `[x, z]`; `None` unless
    /// `cell` is positive and at most the world's bound, `min` is below `max` on both axes, both
    /// are within the bound, and the cells are at most `MAX_CELLS`.
    pub(crate) fn new(cell: Num, min: [Num; 2], max: [Num; 2]) -> Option<Grid> {
        if cell <= Num::ZERO || cell > Position::BOUND {
            return None;
        }
        let mut size = [0; 2];
        for axis in 0..2 {
            let within = -Position::BOUND <= min[axis] && max[axis] <= Position::BOUND;
            if min[axis] >= max[axis] || !within {
                return None;
            }
            let span = (max[axis] - min[axis]).to_bits().unsigned_abs();
            let cells = span.div_ceil(cell.to_bits().unsigned_abs());
            size[axis] = u32::try_from(cells).ok()?;
        }
        if u64::from(size[0]) * u64::from(size[1]) > Grid::MAX_CELLS {
            return None;
        }
        Some(Grid { cell, min, size })
    }

    pub(crate) fn cells(&self) -> usize {
        self.size[0] as usize * self.size[1] as usize
    }

    /// The cell `pos` stands in; `None` off the grid. A point on the line between two cells is
    /// in the one after it.
    pub(crate) fn cell_of(&self, pos: Position) -> Option<usize> {
        let at = pos.get();
        let x = self.index(0, at.x)?;
        let z = self.index(1, at.z)?;
        Some(z * self.size[0] as usize + x)
    }

    /// Calls `reveal` with each row's run of the cells whose centers are within `radius` of `pos`
    /// on the ground plane, exactly, as every range is; rows in order.
    pub(crate) fn spans_within(
        &self,
        pos: Position,
        radius: Num,
        mut reveal: impl FnMut(Range<usize>),
    ) {
        let at = pos.get();
        let cell = self.cell.to_bits();
        // In halves of a bit, so each cell's center, half a cell from its edge, is whole. A
        // position and `min` are within 2⁴⁴ bits of the origin and a center within 2⁴⁶, so every
        // center is within 8 bounds: 2⁴⁸ halves, which leaves i64 room for sums, and i128 for
        // squares.
        let twice = |value: Num| 2 * value.to_bits();
        let reach = twice(radius).min(8 * twice(Position::BOUND));
        let size = self.size.map(i64::from);
        let from = [
            twice(at.x) - twice(self.min[0]),
            twice(at.z) - twice(self.min[1]),
        ];
        let low_row = (from[1] - reach).div_euclid(2 * cell).max(0);
        let high_row = (from[1] + reach).div_euclid(2 * cell).min(size[1] - 1);
        for z in low_row..=high_row {
            let dz = i128::from(from[1] - cell * (2 * z + 1));
            let rest = i128::from(reach).pow(2) - dz * dz;
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

    fn index(&self, axis: usize, at: Num) -> Option<usize> {
        let offset = at.to_bits().checked_sub(self.min[axis].to_bits())?;
        if offset < 0 {
            return None;
        }
        let index = offset / self.cell.to_bits();
        (index < i64::from(self.size[axis])).then(|| usize::try_from(index).expect("a small index"))
    }
}

/// A map's `[grid]`: `cell` in meters, and `min` and `max` as `[x, z]`, refused unless they make a
/// grid.
impl<'de> Deserialize<'de> for Grid {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Grid, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            cell: Scalar,
            min: [Scalar; 2],
            max: [Scalar; 2],
        }
        let fields = Fields::deserialize(deserializer)?;
        let num = |scalar: Scalar| {
            scalar
                .to_num()
                .ok_or_else(|| D::Error::custom("a grid value beyond a Num"))
        };
        let min = [num(fields.min[0])?, num(fields.min[1])?];
        let max = [num(fields.max[0])?, num(fields.max[1])?];
        Grid::new(num(fields.cell)?, min, max).ok_or_else(|| {
            D::Error::custom("a grid needs a positive cell within the bound, min below max, and at most 2²² cells")
        })
    }
}

#[cfg(test)]
mod tests {
    use campfire_math::Vec3;

    use super::*;

    fn num(value: i64) -> Num {
        Num::from_int(value).unwrap()
    }

    fn at(x: Num, z: Num) -> Position {
        Position::new(Vec3::new(x, Num::ZERO, z)).unwrap()
    }

    #[test]
    fn a_grid_covers_its_rectangle_in_whole_cells_and_reveals_exactly() {
        // 1 m cells from (−2, −1) to cover (2, 1.5): 4 along x, 3 along z, the last row half off.
        let half = Num::from_bits(1 << 23);
        let grid = Grid::new(num(1), [num(-2), num(-1)], [num(2), num(1) + half]).unwrap();
        assert_eq!(grid.cells(), 12);
        // Cell (2, 1) is x from 0 to 1, z from 0 to 1: number 1 × 4 + 2 = 6.
        assert_eq!(grid.cell_of(at(Num::ZERO, Num::ZERO)), Some(6));
        assert_eq!(
            grid.cell_of(at(num(1) - Num::EPSILON, num(1) - Num::EPSILON)),
            Some(6)
        );
        assert_eq!(grid.cell_of(at(num(-2), num(-1))), Some(0));
        let off = [
            at(num(2), Num::ZERO),
            at(num(-2) - Num::EPSILON, Num::ZERO),
            at(Num::ZERO, num(2)),
        ];
        assert_eq!(off.map(|pos| grid.cell_of(pos)), [None, None, None]);

        // From (0, 0) the nearest centers, (±0.5, ±0.5), are √0.5 ≈ 0.707 m away, the next
        // ones, such as (1.5, 0.5), √2.5 ≈ 1.58 m. A radius of 1.5 reaches only the four.
        let reveal = |radius: Num| {
            let mut cells = Vec::new();
            grid.spans_within(at(Num::ZERO, Num::ZERO), radius, |span| cells.extend(span));
            cells
        };
        assert_eq!(reveal(num(1) + half), [1, 2, 5, 6]);
        // A radius of 1.59 reaches the centers √2.5 ≈ 1.581 away too: the ring around the four,
        // less the cells off the grid and the corners, √4.5 away.
        let reach = Num::from_bits((159 << 24) / 100);
        assert_eq!(reveal(reach), [0, 1, 2, 3, 4, 5, 6, 7, 9, 10]);
        assert!(reveal(Num::ZERO).is_empty());

        // Against each cell's center tested alone, in halves of a bit, from points on and off
        // the grid, on cell lines and between them, with radii that end on centers and between.
        let quarter = Num::from_bits(1 << 22);
        let twice = |value: Num| 2 * i128::from(value.to_bits());
        for x in -12..12 {
            for z in -8..10 {
                let pos = at(quarter * x, quarter * z);
                for radius in [0, 1, 2, 3, 5, 6, 7, 9, 12, 20].map(|r| quarter * r) {
                    let mut spans = Vec::new();
                    grid.spans_within(pos, radius, |span| spans.extend(span));
                    let alone: Vec<usize> = (0..grid.cells())
                        .filter(|&cell| {
                            let center = |index: usize, axis: usize| {
                                twice(grid.min[axis])
                                    + i128::from(grid.cell.to_bits()) * (2 * index as i128 + 1)
                            };
                            let dx = twice(pos.get().x) - center(cell % 4, 0);
                            let dz = twice(pos.get().z) - center(cell / 4, 1);
                            dx * dx + dz * dz <= twice(radius) * twice(radius)
                        })
                        .collect();
                    assert_eq!(spans, alone, "{x} {z} {radius:?}");
                }
            }
        }

        for (cell, min, max) in [
            (Num::ZERO, [num(0), num(0)], [num(1), num(1)]),
            (
                Position::BOUND + Num::EPSILON,
                [num(0), num(0)],
                [num(1), num(1)],
            ),
            (num(1), [num(1), num(0)], [num(1), num(1)]),
            (num(1), [num(0), num(0)], [num(4096), num(4096) + num(1)]),
        ] {
            assert_eq!(Grid::new(cell, min, max), None, "{cell:?} {min:?} {max:?}");
        }
    }
}
