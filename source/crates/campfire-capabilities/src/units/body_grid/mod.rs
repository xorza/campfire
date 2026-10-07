use campfire_math::Num;
use campfire_sim::{Position, StableId};

use crate::values::row_directory::{RowDirectory, RowEntries};
use crate::values::shape::Shape;

/// The bodies of a stage, as a sorted index of cells of the ground plane: a query of a box visits
/// the bodies of the cells it covers, grown by the widest body's bound, the radius of the least
/// circle that holds it, so it meets every body that may reach into the box, each once, and few
/// others. A sort, not a grid over the map, as a map may be wide and its bodies few, as the
/// broadphase finds its pairs. A cell is twice the widest bound, a meter at least. Its buffer stays between builds, so a build allocates nothing
/// once it has grown, and costs `n log n`; a query finds the bodies of each row of cells it
/// covers from where each row starts, searches only those, and never costs more than a pass over
/// every body. A grid whose rows spread far wider than its bodies keeps no row starts, and a
/// query searches all its bodies for each row.
#[derive(Debug)]
pub(crate) struct BodyGrid<K> {
    /// A cell's side, in raw units.
    cell: i64,
    widest: Num,
    /// Sorted by row, then column, then stable id, and where each row starts, as one layer.
    entries: Vec<GridBody<K>>,
    rows: RowDirectory<()>,
}

/// A body of the grid: its unit, the key its reader finds the unit by, where it stands, its
/// shape and its cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GridBody<K> {
    pub(crate) id: StableId,
    pub(crate) key: K,
    pub(crate) at: Position,
    pub(crate) shape: Shape,
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
            cell: 0,
            widest: Num::ZERO,
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
                row: 0,
                column: 0,
            }));
        self.widest = self
            .entries
            .iter()
            .map(|body| body.shape.bound())
            .max()
            .unwrap_or(Num::ZERO);
        let side = self
            .widest
            .checked_mul_int(2)
            .expect("a body's bound is within 64 m");
        self.cell = side.max(Num::ONE).to_bits();
        let cell = self.cell;
        for body in &mut self.entries {
            let at = body.at.get();
            body.row = at.z.to_bits().div_euclid(cell);
            body.column = at.x.to_bits().div_euclid(cell);
        }
        self.entries
            .sort_unstable_by_key(|body| (body.row, body.column, body.id));
        self.rows
            .rebuild(self.entries.iter().map(|body| ((), body.row)));
    }

    /// Calls `visit` with each body whose bound's disc may reach into the box of the ground plane
    /// from `low` to `high`, `[x, z]` each, each body once, in no order a caller may rely on.
    pub(crate) fn visit(&self, low: [Num; 2], high: [Num; 2], mut visit: impl FnMut(&GridBody<K>)) {
        if self.entries.is_empty() {
            return;
        }
        let grow = self.widest.to_bits();
        let cell = |bits: i64| bits.div_euclid(self.cell);
        let span = |axis: usize| {
            let from = cell(low[axis].to_bits().saturating_sub(grow));
            from..=cell(high[axis].to_bits().saturating_add(grow))
        };
        let (columns, rows) = (span(0), span(1));
        let inside =
            |body: &&GridBody<K>| rows.contains(&body.row) && columns.contains(&body.column);
        // A box over more rows than there are bodies costs less as one pass over them all.
        let row_count = rows.end().abs_diff(*rows.start()).saturating_add(1);
        if row_count > self.entries.len() as u64 {
            self.entries.iter().filter(inside).for_each(visit);
            return;
        }
        let (first, last) = self.rows.span(()).expect("the grid holds bodies");
        for row in *rows.start().max(&first)..=*rows.end().min(&last) {
            let Some(found) = self.rows.row((), row) else {
                continue;
            };
            let (RowEntries::Row(run) | RowEntries::Layer(run)) = found;
            let run = &self.entries[run];
            let start =
                run.partition_point(|body| (body.row, body.column) < (row, *columns.start()));
            let end = run.partition_point(|body| (body.row, body.column) <= (row, *columns.end()));
            run[start..end].iter().for_each(&mut visit);
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
