use std::ops::{Range, RangeInclusive};

use campfire_math::{Flat, FloorRoot, I64x4, Num, Vec3};
use campfire_sim::Position;

use crate::geometry::body_box::BodyBox;
use crate::geometry::bounds::Bounds;
use crate::geometry::halves::Halves;

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
        let step = i128::from(self.cell.to_bits());
        let min = self.bounds.min();
        let index = |index: usize| i128::try_from(index).expect("a cell of the grid");
        let at = Halves::ground(pos);
        let offset = |axis: usize| {
            let center = |index: i128| Halves::of(min[axis]) + step * (2 * index + 1);
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

    /// Calls `mark` with each row's run of the cells whose centers come closer than `reach` to
    /// `body` at `pos` on the ground plane, exactly: a center at `reach` is not closer. A box and
    /// a disc round it are convex, so each row's cells are one run. Rows in order. It tests each
    /// cell of the box's bounding rectangle grown by `reach`, which a body that never moves pays
    /// only as it spawns or goes.
    pub(crate) fn box_spans_closer(
        &self,
        pos: Position,
        body: &BodyBox,
        reach: Num,
        mut mark: impl FnMut(Range<usize>),
    ) {
        let centre = Halves::ground(pos);
        let extent = body.extent();
        let min = self.bounds.min();
        let cell = i128::from(self.cell.to_bits());
        let last = |axis: usize| i128::from(self.size[axis]) - 1;
        // The cells whose centres lie within the rectangle, along `axis`: a centre at
        // `2 min + cell (2 i + 1)` halves.
        let span = |axis: usize| {
            let grow = Halves::of(extent[axis]) + Halves::of(reach);
            let from = centre[axis] - grow - Halves::of(min[axis]) - cell;
            let to = centre[axis] + grow - Halves::of(min[axis]) - cell;
            let first = ceil_div(from, 2 * cell).max(0);
            let end = to.div_euclid(2 * cell).min(last(axis));
            first..=end
        };
        let closer = |off| body.closer_twice(off, reach);
        self.runs(span(1), span(0), centre, closer, &mut mark);
    }

    /// Calls `mark` with each row's run of the cells whose squares a box's inside shares a point
    /// with, `body` at `pos`: the cells it covers, touching not counted. Rows in order.
    pub(crate) fn box_covers(
        &self,
        pos: Position,
        body: &BodyBox,
        mut mark: impl FnMut(Range<usize>),
    ) {
        let centre = Halves::ground(pos);
        let extent = body.extent();
        let min = self.bounds.min();
        let cell = i128::from(self.cell.to_bits());
        let last = |axis: usize| i128::from(self.size[axis]) - 1;
        // The cells whose squares, from `2 min + 2 cell i` to the next, the rectangle meets.
        let span = |axis: usize| {
            let from = centre[axis] - Halves::of(extent[axis]) - Halves::of(min[axis]);
            let to = centre[axis] + Halves::of(extent[axis]) - Halves::of(min[axis]);
            let first = from.div_euclid(2 * cell).max(0);
            let end = to.div_euclid(2 * cell).min(last(axis));
            first..=end
        };
        let covers = |off| body.overlaps_square_twice(off, cell);
        self.runs(span(1), span(0), centre, covers, &mut mark);
    }

    /// Calls `mark` with each of `rows`' run of the cells of `columns` whose centres, as offsets
    /// from `centre` in halves of a bit, `hit` holds of: a convex shape's cells of a row are one
    /// run, which ends at the first cell past it. Rows in order.
    fn runs(
        &self,
        rows: RangeInclusive<i128>,
        columns: RangeInclusive<i128>,
        centre: Flat,
        hit: impl Fn(Flat) -> bool,
        mark: &mut impl FnMut(Range<usize>),
    ) {
        let index = |value: i128| usize::try_from(value).expect("a cell of the grid");
        for row in rows {
            let mut run: Option<(usize, usize)> = None;
            for column in columns.clone() {
                let cell_at = index(row) * self.columns() + index(column);
                if hit(self.center_twice(cell_at) - centre) {
                    run = Some(run.map_or((cell_at, cell_at), |(first, _)| (first, cell_at)));
                } else if run.is_some() {
                    break;
                }
            }
            if let Some((first, held)) = run {
                mark(first..held + 1);
            }
        }
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
        let twice = |value: Num| {
            i64::try_from(Halves::of(value)).expect("a coordinate within the bound fits i64")
        };
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
            let open = zero.simd_le(rest).to_bits();
            let (low, high) = (low.to_array(), high.to_array());
            for (lane, row) in (z..=last).take(4).enumerate() {
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
    pub(crate) fn center_twice(&self, cell: usize) -> Flat {
        let at = [cell % self.columns(), cell / self.columns()];
        let step = i128::from(self.cell.to_bits());
        let min = self.bounds.min();
        Flat::from_array([0, 1].map(|axis| {
            let index = i128::try_from(at[axis]).expect("a cell of the grid");
            Halves::of(min[axis]) + step * (2 * index + 1)
        }))
    }

    /// Whether `hit` is true of a cell of the grid whose closed square the segment from `from` to
    /// `to` on the ground plane touches, exactly, corners included, so a segment through a corner
    /// touches the four cells round it; cells are visited column by column, and the first hit
    /// ends the visit. Each column's span of the segment gives the rows it touches, from the
    /// segment's z at the span's ends, a fraction kept whole by its denominator. The z at each
    /// line between columns grows by the same step, so its quotient by the denominator carries
    /// from one column to the next: a segment divides at most four times, however many columns
    /// it crosses.
    pub(crate) fn touches(
        &self,
        from: Position,
        to: Position,
        mut hit: impl FnMut(usize) -> bool,
    ) -> bool {
        let columns = self.columns();
        self.touches_columns(from, to, |column, rows| {
            rows.into_iter().any(|row| hit(row * columns + column))
        })
    }

    /// The cells `touches` visits, a column at a time, in its order: whether `hit` is true of a
    /// column and its run of rows, empty in a column the segment touches no row of.
    pub(crate) fn touches_columns(
        &self,
        from: Position,
        to: Position,
        mut hit: impl FnMut(usize, Range<usize>) -> bool,
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
        let index = |at: i128| usize::try_from(at).expect("a cell of the grid");
        let mut rows = |column: i128, low: Quotient, high: Quotient| {
            let first = index((low.ceil() - 1).max(0));
            let end = index((high.floor.min(last(1)) + 1).max(0));
            hit(index(column), first..end.max(first))
        };
        if dx == 0 {
            let (low, high) = (a[1].min(b[1]), a[1].max(b[1]));
            let (low, high) = (Quotient::of(low, cell), Quotient::of(high, cell));
            return (first_column..=last_column).any(|column| rows(column, low, high));
        }
        // The segment's z at `x`, times `dx`, over `scale`: a column's span runs from the line
        // before it, or `a`, to the line after it, or `b`. A segment that starts or ends past the
        // grid's columns spans the first or the last column only to that column's line.
        let scale = dx * cell;
        let at = |x: i128| Quotient::of(a[1] * dx + (x - a[0]) * dz, scale);
        let step = Quotient::of(cell * dz, scale);
        let mut line = at((first_column + 1) * cell);
        let mut start = at(a[0].max(first_column * cell));
        let end = at(b[0].min((last_column + 1) * cell));
        for column in first_column..=last_column {
            let finish = if column == last_column { end } else { line };
            if rows(column, start.min(finish), start.max(finish)) {
                return true;
            }
            start = line;
            line = line.plus(step, scale);
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

/// A value over a positive divisor: the floor of their quotient, and what remains, from 0 to
/// below the divisor, so two values over one divisor order as their quotients do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Quotient {
    floor: i128,
    rest: i128,
}

impl Quotient {
    /// `value` over `by`, which is positive.
    const fn of(value: i128, by: i128) -> Quotient {
        let floor = value.div_euclid(by);
        Quotient {
            floor,
            rest: value - floor * by,
        }
    }

    /// The quotient rounded up.
    const fn ceil(self) -> i128 {
        self.floor + (self.rest != 0) as i128
    }

    /// The sum of the values of `self` and `other`, over the same `by`.
    const fn plus(self, other: Quotient, by: i128) -> Quotient {
        let rest = self.rest + other.rest;
        let carry = (rest >= by) as i128;
        Quotient {
            floor: self.floor + other.floor + carry,
            rest: rest - carry * by,
        }
    }
}

/// `value` over `by`, positive, rounded up.
const fn ceil_div(value: i128, by: i128) -> i128 {
    -(-value).div_euclid(by)
}

#[cfg(test)]
mod tests;
