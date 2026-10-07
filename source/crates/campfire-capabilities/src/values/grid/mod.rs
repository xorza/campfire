use std::ops::Range;

use campfire_math::{FloorRoot, I64x4, Num, Vec3};
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
    /// square below `reach²` is one at most `reach² − 1`. Four rows a step in lanes when every
    /// square fits them, else one row at a time.
    fn spans(
        &self,
        pos: Position,
        radius: Num,
        strict: bool,
        mut reveal: impl FnMut(Range<usize>),
    ) {
        let rows = self.disc_rows(pos, radius, strict);
        if rows.fit_lanes(self.cell.to_bits()) {
            self.rows_in_lanes(&rows, &mut reveal);
        } else {
            self.rows_one_by_one(&rows, &mut reveal);
        }
    }

    /// The rows a disc of `radius` around `pos` may reach, in halves of a bit.
    fn disc_rows(&self, pos: Position, radius: Num, strict: bool) -> Rows {
        let at = pos.get();
        let cell = self.cell.to_bits();
        // In halves of a bit, so each cell's center, half a cell from its edge, is whole. A
        // position and `min` are within 2⁴⁴ bits of the origin and a center within 2⁴⁶, so every
        // center is within 8 bounds: 2⁴⁸ halves, which leaves i64 room for sums, and i128 for
        // squares.
        let twice = |value: Num| 2 * value.to_bits();
        let reach = 2 * radius.to_bits().min(8 * Position::BOUND.to_bits());
        let min = self.bounds.min();
        let from = [twice(at.x) - twice(min[0]), twice(at.z) - twice(min[1])];
        Rows {
            from,
            reach,
            strict,
            first: (from[1] - reach).div_euclid(2 * cell).max(0),
            last: (from[1] + reach)
                .div_euclid(2 * cell)
                .min(i64::from(self.size[1]) - 1),
        }
    }

    fn rows_one_by_one(&self, rows: &Rows, reveal: &mut impl FnMut(Range<usize>)) {
        let cell = self.cell.to_bits();
        let Rows {
            from,
            reach,
            strict,
            first,
            last,
        } = *rows;
        for z in first..=last {
            let dz = i128::from(from[1] - cell * (2 * z + 1));
            let rest = i128::from(reach).pow(2) - i128::from(strict) - dz * dz;
            if rest < 0 {
                continue;
            }
            let half =
                i64::try_from(rest.cast_unsigned().floor_root()).expect("a root within reach");
            // A center at 2x + 1 half cells is within `half` of `from[0]`.
            let low_odd = -(half - from[0]).div_euclid(cell);
            let high_odd = (from[0] + half).div_euclid(cell);
            let low = (-(1 - low_odd).div_euclid(2)).max(0);
            let high = (high_odd - 1)
                .div_euclid(2)
                .min(i64::from(self.size[0]) - 1);
            self.reveal_run(z, low, high, reveal);
        }
    }

    /// The rows of `rows_one_by_one`, four a step. The center of column x lies within `half` of
    /// `from[0]` when x is from ⌈(from₀ − half − cell) / 2·cell⌉ to ⌊(from₀ + half − cell) / 2·cell⌋.
    fn rows_in_lanes(&self, rows: &Rows, reveal: &mut impl FnMut(Range<usize>)) {
        let cell = self.cell.to_bits();
        let Rows {
            from,
            reach,
            strict,
            first,
            last,
        } = *rows;
        let pitch = 2 * cell;
        let zero = I64x4::splat(0);
        let top = I64x4::splat(reach * reach - i64::from(strict));
        let along = I64x4::from_array([0, pitch, 2 * pitch, 3 * pitch]);
        let right = I64x4::splat(i64::from(self.size[0]) - 1);
        let mut z = first;
        while z <= last {
            let dz = I64x4::splat(from[1] - cell * (2 * z + 1)).wrapping_sub(along);
            let rest = top.wrapping_sub(dz.mul_narrow(dz));
            let half = rest.max(zero).cast_unsigned().floor_root().cast_signed();
            let low = zero
                .wrapping_sub(
                    half.wrapping_add(I64x4::splat(cell - from[0]))
                        .div_euclid(pitch),
                )
                .max(zero);
            let high = half
                .wrapping_add(I64x4::splat(from[0] - cell))
                .div_euclid(pitch)
                .min(right);
            let left = last - z + 1;
            let open = zero.simd_le(rest).and(low.simd_le(high)).to_bits()
                & if left < 4 { (1 << left) - 1 } else { 0b1111 };
            let (low, high) = (low.to_array(), high.to_array());
            for (lane, row) in (z..).take(4).enumerate() {
                if open >> lane & 1 == 1 {
                    self.reveal_run(row, low[lane], high[lane], reveal);
                }
            }
            z += 4;
        }
    }

    /// Calls `reveal` with the cells of row `z` from column `low` to `high`, when they are any.
    fn reveal_run(&self, z: i64, low: i64, high: i64, reveal: &mut impl FnMut(Range<usize>)) {
        if low <= high {
            let start = z * i64::from(self.size[0]);
            let cells = |x: i64| usize::try_from(start + x).expect("a cell of the grid");
            reveal(cells(low)..cells(high) + 1);
        }
    }

    /// The center of `cell`, `[x, z]` in halves of a bit, exactly.
    pub(crate) fn center_twice(&self, cell: usize) -> [i128; 2] {
        let at = [cell % self.columns(), cell / self.columns()];
        let step = i128::from(self.cell.to_bits());
        let min = self.bounds.min();
        [0, 1].map(|axis| {
            let index = i128::try_from(at[axis]).expect("a cell of the grid");
            2 * i128::from(min[axis].to_bits()) + step * (2 * index + 1)
        })
    }

    /// Whether `hit` is true of a cell of the grid whose closed square the segment from `from` to
    /// `to` on the ground plane touches, exactly, corners included, so a segment through a corner
    /// touches the four cells round it; cells are visited column by column, and the first hit
    /// ends the visit. Each column's span of the segment gives the rows it touches, from the
    /// segment's z at the span's ends, a fraction kept whole by its denominator.
    pub(crate) fn touches(
        &self,
        from: Position,
        to: Position,
        mut hit: impl FnMut(usize) -> bool,
    ) -> bool {
        let min = self.bounds.min().map(|axis| i128::from(axis.to_bits()));
        let ground = |pos: Position| {
            let at = pos.get();
            [
                i128::from(at.x.to_bits()) - min[0],
                i128::from(at.z.to_bits()) - min[1],
            ]
        };
        let (mut a, mut b) = (ground(from), ground(to));
        if a[0] > b[0] {
            (a, b) = (b, a);
        }
        let cell = i128::from(self.cell.to_bits());
        let last = |axis: usize| i128::from(self.size[axis]) - 1;
        let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
        let first_column = (ceil_div(a[0], cell) - 1).max(0);
        let last_column = b[0].div_euclid(cell).min(last(0));
        for column in first_column..=last_column {
            let (z_low, z_high, scale) = if dx == 0 {
                (a[1].min(b[1]), a[1].max(b[1]), cell)
            } else {
                // The segment's z at x, times `dx`.
                let at = |x: i128| a[1] * dx + (x - a[0]) * dz;
                let start = at(a[0].max(column * cell));
                let end = at(b[0].min((column + 1) * cell));
                (start.min(end), start.max(end), dx * cell)
            };
            let first_row = (ceil_div(z_low, scale) - 1).max(0);
            let last_row = z_high.div_euclid(scale).min(last(1));
            for row in first_row..=last_row {
                let at = row * i128::from(self.size[0]) + column;
                if hit(usize::try_from(at).expect("a cell of the grid")) {
                    return true;
                }
            }
        }
        false
    }

    /// The index along `axis`, 0 for x and 1 for z, of `at`, which is within the bounds.
    pub(crate) fn index(&self, axis: usize, at: Num) -> usize {
        let offset = at.to_bits() - self.bounds.min()[axis].to_bits();
        let index = (offset / self.cell.to_bits()).min(i64::from(self.size[axis]) - 1);
        usize::try_from(index).expect("a point within the bounds is past their min")
    }
}

/// The rows a disc may reach on a grid, in halves of a bit: its center from the grid's min, its
/// reach, whether a center at the reach is out, and the first and last row.
#[derive(Debug, Clone, Copy)]
struct Rows {
    from: [i64; 2],
    reach: i64,
    strict: bool,
    first: i64,
    last: i64,
}

impl Rows {
    /// Whether every lane of `Grid::rows_in_lanes` stays in its domain on cells of `cell` bits: a
    /// row past the last of a step lies at most 7 cells beyond the reach, so each offset fits an
    /// `i32` and each square 62 bits.
    const fn fit_lanes(&self, cell: i64) -> bool {
        self.reach + 8 * cell < 1 << 31
    }
}

/// `value` over `by`, positive, rounded up.
const fn ceil_div(value: i128, by: i128) -> i128 {
    -(-value).div_euclid(by)
}

#[cfg(test)]
mod tests;
