use std::ops::ControlFlow;

use campfire_math::{Num, Vec3};

use crate::cell_key::CellKey;
use crate::cell_levels::CellLevels;
use crate::placed::Placed;
use crate::row_index::{RowEntries, RowIndex};

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
pub struct BodyGrid<K, I> {
    levels: CellLevels,
    /// Each level that holds a body, in order, with its widest bound.
    widest: Vec<LevelWidest>,
    /// Sorted by level, row, column, then id, and where each level's rows start.
    entries: Vec<Indexed<K, I>>,
    /// Each entry's cell, beside it: a search runs over these dense keys, not over the bodies.
    cells: Vec<CellKey>,
    rows: RowIndex<u8>,
    /// The bodies of a build and their order, kept between builds.
    placed: Vec<Indexed<K, I>>,
    order: Vec<Order>,
}

/// A level that holds a body, and the widest bound of its bodies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LevelWidest {
    level: u8,
    widest: Num,
}

/// A body to index: its unit's id, whose number orders it, the key its reader finds the unit by,
/// and the body where it
/// stands, a point for a unit with no body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Indexed<K, I> {
    pub id: I,
    pub key: K,
    pub body: Placed,
}

/// A body's place in a build's order: its cell's key above its id's number, so a sort compares one
/// number, and where it lies among the bodies placed. A sort of these moves 32 bytes an entry,
/// where one of the bodies moves each whole body. Every cell is of layer 0.
#[derive(Debug, Clone, Copy)]
struct Order {
    key: u128,
    placed: u32,
}

impl Order {
    /// The cell of its key, the high half.
    const fn cell(self) -> CellKey {
        CellKey::from_bits((self.key >> 64) as u64)
    }
}

impl<K, I> Default for BodyGrid<K, I> {
    fn default() -> Self {
        Self {
            levels: CellLevels::new(Num::ONE),
            widest: Vec::new(),
            entries: Vec::new(),
            cells: Vec::new(),
            rows: RowIndex::default(),
            placed: Vec::new(),
            order: Vec::new(),
        }
    }
}

impl<K: Copy, I: Copy + Into<u64>> BodyGrid<K, I> {
    /// Indexes `bodies` in place of what it held.
    pub fn rebuild(&mut self, bodies: impl IntoIterator<Item = Indexed<K, I>>) {
        self.placed.clear();
        self.placed.extend(bodies);
        self.index_placed();
    }

    /// Indexes the bodies `placed` holds; inlined, as `rebuild` runs 1.9 % slower with a call.
    #[inline(always)]
    fn index_placed(&mut self) {
        let narrowest = self
            .placed
            .iter()
            .map(|entry| entry.body.shape.bound())
            .min();
        let side = narrowest
            .unwrap_or(Num::ZERO)
            .checked_mul_int(2)
            .expect("a body's bound is within 2,048 m");
        self.levels = CellLevels::new(side.max(Num::ONE));
        let levels = self.levels;
        self.order.clear();
        self.order
            .extend(self.placed.iter().enumerate().map(|(placed, entry)| {
                let at = entry.body.position;
                let level = levels.level(entry.body.shape.bound());
                let (row, column) = (at.z.to_bits(), at.x.to_bits());
                let (row, column) = (levels.index(level, row), levels.index(level, column));
                let cell = CellKey::new(0, level, row, column);
                Order {
                    key: (u128::from(cell.to_bits()) << 64) | u128::from(entry.id.into()),
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
            .extend(self.order.iter().map(|order| order.cell()));
        self.widest.clear();
        for (entry, &cell) in self.entries.iter().zip(&self.cells) {
            let (level, bound) = (cell.level(), entry.body.shape.bound());
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
    /// from `low` to `high`, `[x, z]` each, each body once, in no order a caller may rely on, until
    /// it breaks.
    pub fn visit(
        &self,
        low: [Num; 2],
        high: [Num; 2],
        mut visit: impl FnMut(&Indexed<K, I>) -> ControlFlow<()>,
    ) {
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
                let stopped = self.entries[run.clone()]
                    .iter()
                    .zip(&self.cells[run])
                    .filter(|&(_, &cell)| inside(cell))
                    .try_for_each(|(body, _)| visit(body));
                if stopped.is_break() {
                    return;
                }
                continue;
            }
            for row in *covered.start().max(&rows.first())..=*covered.end().min(&rows.last()) {
                let Some(found) = rows.row(row) else {
                    continue;
                };
                let (RowEntries::Row(run) | RowEntries::Layer(run)) = found;
                let cells = &self.cells[run.clone()];
                let (first, last) = (
                    CellKey::clamped(0, level, row, *columns.start()),
                    CellKey::clamped(0, level, row, *columns.end()),
                );
                let start = cells.partition_point(|&cell| cell < first);
                let end = start + cells[start..].partition_point(|&cell| cell <= last);
                let stopped = self.entries[run.start + start..run.start + end]
                    .iter()
                    .try_for_each(&mut visit);
                if stopped.is_break() {
                    return;
                }
            }
        }
    }

    /// Calls `visit` with each body whose bound's disc may come within `reach` of `position` on the
    /// ground plane, as `visit` does.
    pub fn visit_near(
        &self,
        position: Vec3,
        reach: Num,
        visit: impl FnMut(&Indexed<K, I>) -> ControlFlow<()>,
    ) {
        let low = [position.x, position.z].map(|axis| axis.checked_sub(reach).unwrap_or(Num::MIN));
        let high = [position.x, position.z].map(|axis| axis.checked_add(reach).unwrap_or(Num::MAX));
        self.visit(low, high, visit);
    }
}

impl<K: Copy + PartialEq, I: Copy + PartialEq + Into<u64>> BodyGrid<K, I> {
    /// Indexes `bodies` in place of what it held, as `rebuild` does, but keeps the index as it is
    /// when they are the bodies it holds, in the same order: for a caller whose bodies often stand
    /// still between two builds, as a read that no move came before.
    pub fn refresh(&mut self, bodies: impl IntoIterator<Item = Indexed<K, I>>) {
        let mut bodies = bodies.into_iter();
        let mut held = 0;
        let differs = bodies.find(|body| {
            let same = self.placed.get(held) == Some(body);
            held += usize::from(same);
            !same
        });
        if differs.is_none() && held == self.placed.len() {
            return;
        }
        self.placed.truncate(held);
        self.placed.extend(differs);
        self.placed.extend(bodies);
        self.index_placed();
    }
}

#[cfg(test)]
mod tests;
