use campfire_math::Num;
use campfire_sim::{Position, StableId};

use crate::geometry::cell_levels::CellLevels;
use crate::geometry::shape::Shape;
use crate::values::row_directory::{RowDirectory, RowEntries};

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
    entries: Vec<GridBody<K>>,
    rows: RowDirectory<u8>,
}

/// A level that holds a body, and the widest bound of its bodies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LevelWidest {
    level: u8,
    widest: Num,
}

/// A body of the grid: its unit, the key its reader finds the unit by, where it stands, its
/// shape and its cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GridBody<K> {
    pub(crate) id: StableId,
    pub(crate) key: K,
    pub(crate) at: Position,
    pub(crate) shape: Shape,
    level: u8,
    row: i64,
    column: i64,
}

/// A body to index: its unit, the key its reader finds the unit by, where it stands, and its
/// shape, a point's for a unit with no body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Placed<K> {
    pub(crate) id: StableId,
    pub(crate) key: K,
    pub(crate) at: Position,
    pub(crate) shape: Shape,
}

impl<K> Default for BodyGrid<K> {
    fn default() -> BodyGrid<K> {
        BodyGrid {
            levels: CellLevels::new(Num::ONE),
            widest: Vec::new(),
            entries: Vec::new(),
            rows: RowDirectory::default(),
        }
    }
}

impl<K: Copy> BodyGrid<K> {
    /// Indexes `bodies` in place of what it held.
    pub(crate) fn rebuild(&mut self, bodies: impl IntoIterator<Item = Placed<K>>) {
        self.entries.clear();
        self.entries
            .extend(bodies.into_iter().map(|placed| GridBody {
                id: placed.id,
                key: placed.key,
                at: placed.at,
                shape: placed.shape,
                level: 0,
                row: 0,
                column: 0,
            }));
        let narrowest = self.entries.iter().map(|body| body.shape.bound()).min();
        let side = narrowest
            .unwrap_or(Num::ZERO)
            .checked_mul_int(2)
            .expect("a body's bound is within 2,048 m");
        self.levels = CellLevels::new(side.max(Num::ONE));
        let levels = self.levels;
        for body in &mut self.entries {
            let at = body.at.get();
            body.level = levels.level(body.shape.bound());
            body.row = levels.index(body.level, at.z.to_bits());
            body.column = levels.index(body.level, at.x.to_bits());
        }
        self.entries
            .sort_unstable_by_key(|body| (body.level, body.row, body.column, body.id));
        self.widest.clear();
        for body in &self.entries {
            let bound = body.shape.bound();
            match self.widest.last_mut() {
                Some(last) if last.level == body.level => last.widest = last.widest.max(bound),
                _ => self.widest.push(LevelWidest {
                    level: body.level,
                    widest: bound,
                }),
            }
        }
        self.rows
            .rebuild(self.entries.iter().map(|body| (body.level, body.row)));
    }

    /// Calls `visit` with each body whose bound's disc may reach into the box of the ground plane
    /// from `low` to `high`, `[x, z]` each, each body once, in no order a caller may rely on.
    pub(crate) fn visit(&self, low: [Num; 2], high: [Num; 2], mut visit: impl FnMut(&GridBody<K>)) {
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
                self.entries[run]
                    .iter()
                    .filter(|body| covered.contains(&body.row) && columns.contains(&body.column))
                    .for_each(&mut visit);
                continue;
            }
            for row in *covered.start().max(&rows.first())..=*covered.end().min(&rows.last()) {
                let Some(found) = rows.row(row) else {
                    continue;
                };
                let (RowEntries::Row(run) | RowEntries::Layer(run)) = found;
                let run = &self.entries[run];
                let start =
                    run.partition_point(|body| (body.row, body.column) < (row, *columns.start()));
                let end =
                    run.partition_point(|body| (body.row, body.column) <= (row, *columns.end()));
                run[start..end].iter().for_each(&mut visit);
            }
        }
    }

    /// Calls `visit` with each body whose bound's disc may come within `reach` of `at` on the
    /// ground plane, as `visit` does.
    pub(crate) fn visit_near(&self, at: Position, reach: Num, visit: impl FnMut(&GridBody<K>)) {
        let at = at.get();
        let low = [at.x, at.z].map(|axis| axis.checked_sub(reach).unwrap_or(Num::MIN));
        let high = [at.x, at.z].map(|axis| axis.checked_add(reach).unwrap_or(Num::MAX));
        self.visit(low, high, visit);
    }
}

#[cfg(test)]
mod tests;
