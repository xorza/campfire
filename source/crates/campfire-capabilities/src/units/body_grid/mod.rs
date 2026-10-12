use campfire_math::Num;
use campfire_sim::{Position, StableId};

use crate::geometry::cell_levels::CellLevels;
use crate::geometry::shape::Shape;
use crate::values::row_directory::{RowDirectory, RowEntries};

#[cfg(feature = "internals")]
pub(crate) mod bench;

/// The bodies of a stage, as a sorted index of cells of the ground plane in levels by size: a
/// body sits in the cell of its center at the least level whose cell is twice its bound, the
/// radius of the least circle that holds it, and a query of a box visits, at each level that
/// holds a body, the bodies of the cells it covers grown by that level's widest bound, so it meets
/// every body that may reach into the box, each once, and few others; a wide body makes only its
/// own level's cells wide. The base cell is twice the narrowest bound, a meter at least. A sort,
/// not a grid over the map, as a map may be wide and its bodies few, as the broadphase finds its
/// pairs. Its buffers stay between builds, so a build allocates nothing once they have grown, and
/// costs `n log n`; a query finds the bodies of each row of cells it covers from where each row
/// starts, searches only those, and never costs more than a pass over each level's bodies. A level
/// whose rows spread far wider than its bodies keeps no row starts, and a query searches all its
/// bodies for each row.
#[derive(Debug)]
pub(crate) struct BodyGrid<K> {
    levels: CellLevels,
    /// Each level that holds a body, in order, with its widest bound.
    widest: Vec<LevelWidest>,
    /// Sorted by level, row, column, then stable id, and where each level's rows start.
    entries: Vec<Placed<K>>,
    /// Each entry's cell, beside it: a search runs over these dense keys, not over the bodies.
    cells: Vec<CellKey>,
    rows: RowDirectory<u8>,
    /// The bodies of a build and their order, kept between builds.
    placed: Vec<Placed<K>>,
    order: Vec<Order>,
}

/// A level that holds a body, and the widest bound of its bodies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LevelWidest {
    level: u8,
    widest: Num,
}

/// A body to index: its unit, the key its reader finds the unit by, where it stands, and its
/// shape, a point's for a unit with no body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Placed<K> {
    pub(crate) id: StableId,
    pub(crate) key: K,
    pub(crate) pos: Position,
    pub(crate) shape: Shape,
}

/// A body's place in a build's order: its cell's key above its stable id, so a sort compares one
/// number, and where it lies among the bodies placed. A sort of these moves 32 bytes an entry,
/// where one of the bodies moves each whole body.
#[derive(Debug, Clone, Copy)]
struct Order {
    key: u128,
    placed: u32,
}

/// A cell of a level, packed so that cells order as their level, row and column do: the level
/// in the top 8 bits, the row and the column, each offset by 2²⁷, in 28 bits each. A body's
/// rows and columns lie within ±2²⁰, as positions lie within ±2⁴⁴ bits and a cell is at least a
/// meter, 2²⁴ bits; a search's rows and columns past ±2²⁷ clamp to the ends, where no body is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct CellKey(u64);

impl CellKey {
    /// The offset of a row or a column.
    const MIDDLE: i64 = 1 << 27;
    const MASK: u64 = (1 << 28) - 1;

    const fn new(level: u8, row: i64, column: i64) -> CellKey {
        CellKey((level as u64) << 56 | CellKey::place(row) << 28 | CellKey::place(column))
    }

    /// A row or a column, clamped to its 28 bits and offset into them.
    const fn place(index: i64) -> u64 {
        let index = if index < -CellKey::MIDDLE {
            -CellKey::MIDDLE
        } else if index >= CellKey::MIDDLE {
            CellKey::MIDDLE - 1
        } else {
            index
        };
        (index + CellKey::MIDDLE).cast_unsigned()
    }

    /// The cell of an `Order`'s key, its high half.
    const fn of_order(key: u128) -> CellKey {
        CellKey((key >> 64) as u64)
    }

    const fn level(self) -> u8 {
        (self.0 >> 56) as u8
    }

    const fn row(self) -> i64 {
        ((self.0 >> 28) & CellKey::MASK).cast_signed() - CellKey::MIDDLE
    }

    const fn column(self) -> i64 {
        (self.0 & CellKey::MASK).cast_signed() - CellKey::MIDDLE
    }
}

impl<K> Default for BodyGrid<K> {
    fn default() -> BodyGrid<K> {
        BodyGrid {
            levels: CellLevels::new(Num::ONE),
            widest: Vec::new(),
            entries: Vec::new(),
            cells: Vec::new(),
            rows: RowDirectory::default(),
            placed: Vec::new(),
            order: Vec::new(),
        }
    }
}

impl<K: Copy> BodyGrid<K> {
    /// Indexes `bodies` in place of what it held.
    pub(crate) fn rebuild(&mut self, bodies: impl IntoIterator<Item = Placed<K>>) {
        self.placed.clear();
        self.placed.extend(bodies);
        let narrowest = self.placed.iter().map(|body| body.shape.bound()).min();
        let side = narrowest
            .unwrap_or(Num::ZERO)
            .checked_mul_int(2)
            .expect("a body's bound is within 2,048 m");
        self.levels = CellLevels::new(side.max(Num::ONE));
        let levels = self.levels;
        self.order.clear();
        self.order
            .extend(self.placed.iter().enumerate().map(|(placed, body)| {
                let at = body.pos.get();
                let level = levels.level(body.shape.bound());
                let (row, column) = (at.z.to_bits(), at.x.to_bits());
                let (row, column) = (levels.index(level, row), levels.index(level, column));
                debug_assert!(row.abs() < CellKey::MIDDLE && column.abs() < CellKey::MIDDLE);
                let cell = CellKey::new(level, row, column);
                Order {
                    key: u128::from(cell.0) << 64 | u128::from(body.id.get()),
                    placed: u32::try_from(placed).expect("bodies fit u32"),
                }
            }));
        self.order.sort_unstable_by_key(|order| order.key);
        let placed = &self.placed;
        self.entries.clear();
        self.entries
            .extend(self.order.iter().map(|order| placed[order.placed as usize]));
        self.cells.clear();
        self.cells
            .extend(self.order.iter().map(|order| CellKey::of_order(order.key)));
        self.widest.clear();
        for (body, &cell) in self.entries.iter().zip(&self.cells) {
            let (level, bound) = (cell.level(), body.shape.bound());
            match self.widest.last_mut() {
                Some(last) if last.level == level => last.widest = last.widest.max(bound),
                _ => self.widest.push(LevelWidest {
                    level,
                    widest: bound,
                }),
            }
        }
        let rows = self.cells.iter().map(|cell| (cell.level(), cell.row()));
        self.rows.rebuild(rows);
    }

    /// Calls `visit` with each body whose bound's disc may reach into the box of the ground plane
    /// from `low` to `high`, `[x, z]` each, each body once, in no order a caller may rely on.
    pub(crate) fn visit(&self, low: [Num; 2], high: [Num; 2], mut visit: impl FnMut(&Placed<K>)) {
        for (rows, &LevelWidest { level, widest }) in self.rows.layers().zip(&self.widest) {
            debug_assert_eq!(rows.layer(), level);
            let grow = widest.to_bits();
            let cell = |bits: i64| self.levels.index(level, bits);
            let span = |axis: usize| {
                let from = cell(low[axis].to_bits().saturating_sub(grow));
                from..=cell(high[axis].to_bits().saturating_add(grow))
            };
            let (columns, covered) = (span(0), span(1));
            let run = rows.entries();
            // A box over more rows than the level has bodies costs less as one pass over them.
            let row_count = covered.end().abs_diff(*covered.start()).saturating_add(1);
            if row_count > run.len() as u64 {
                let inside = |cell: CellKey| {
                    covered.contains(&cell.row()) && columns.contains(&cell.column())
                };
                self.entries[run.clone()]
                    .iter()
                    .zip(&self.cells[run])
                    .filter(|&(_, &cell)| inside(cell))
                    .for_each(|(body, _)| visit(body));
                continue;
            }
            for row in *covered.start().max(&rows.first())..=*covered.end().min(&rows.last()) {
                let Some(found) = rows.row(row) else {
                    continue;
                };
                let (RowEntries::Row(run) | RowEntries::Layer(run)) = found;
                let cells = &self.cells[run.clone()];
                let (first, last) = (
                    CellKey::new(level, row, *columns.start()),
                    CellKey::new(level, row, *columns.end()),
                );
                let start = cells.partition_point(|&cell| cell < first);
                let end = start + cells[start..].partition_point(|&cell| cell <= last);
                self.entries[run.start + start..run.start + end]
                    .iter()
                    .for_each(&mut visit);
            }
        }
    }

    /// Calls `visit` with each body whose bound's disc may come within `reach` of `pos` on the
    /// ground plane, as `visit` does.
    pub(crate) fn visit_near(&self, pos: Position, reach: Num, visit: impl FnMut(&Placed<K>)) {
        let pos = pos.get();
        let low = [pos.x, pos.z].map(|axis| axis.checked_sub(reach).unwrap_or(Num::MIN));
        let high = [pos.x, pos.z].map(|axis| axis.checked_add(reach).unwrap_or(Num::MAX));
        self.visit(low, high, visit);
    }
}

#[cfg(test)]
mod tests;
