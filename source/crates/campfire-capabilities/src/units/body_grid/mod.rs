use campfire_math::Num;
use campfire_sim::{Position, StableId};

/// The bodies of a stage, as a sorted index of cells of the ground plane: a query of a box visits
/// the bodies of the cells it covers, grown by the widest body, so it meets every body that may
/// reach into the box, each once, and few others. A sort, not a grid over the map, as a map may
/// be wide and its bodies few, as the broadphase finds its pairs. A cell is twice the widest
/// body's radius, a meter at least. Its buffer stays between builds, so a build allocates nothing
/// once it has grown, and costs `n log n`; a query finds the bodies of each row of cells it
/// covers from where each row starts, searches only those, and never costs more than a pass over
/// every body. A grid whose rows spread far wider than its bodies keeps no row starts, and a
/// query searches all its bodies for each row.
#[derive(Debug)]
pub(crate) struct BodyGrid<K> {
    /// A cell's side, in raw units.
    cell: i64,
    widest: Num,
    /// Sorted by row, then column, then stable id.
    entries: Vec<GridBody<K>>,
    /// The first entry of each row from `first_row` to `last_row`, the bodies' first and last,
    /// and one past the last row's last; empty when it keeps none.
    first_row: i64,
    last_row: i64,
    starts: Vec<u32>,
}

/// A body of the grid: its unit, the key its reader finds the unit by, where it stands, its
/// radius and its cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GridBody<K> {
    pub(crate) id: StableId,
    pub(crate) key: K,
    pub(crate) at: Position,
    pub(crate) radius: Num,
    row: i64,
    column: i64,
}

/// A body to index: its unit, the key its reader finds the unit by, where it stands, and its
/// radius, 0 for a point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Placed<K> {
    pub(crate) id: StableId,
    pub(crate) key: K,
    pub(crate) at: Position,
    pub(crate) radius: Num,
}

impl<K> Default for BodyGrid<K> {
    fn default() -> BodyGrid<K> {
        BodyGrid {
            cell: 0,
            widest: Num::ZERO,
            entries: Vec::new(),
            first_row: 0,
            last_row: 0,
            starts: Vec::new(),
        }
    }
}

impl<K: Copy> BodyGrid<K> {
    /// How much wider than its bodies the rows may spread for the grid to keep their starts.
    const SPREAD: u64 = 4;
    /// The rows the grid may always keep the starts of.
    const ROWS: u64 = 64;

    /// Indexes `bodies` in place of what it held.
    pub(crate) fn rebuild(&mut self, bodies: impl IntoIterator<Item = Placed<K>>) {
        self.entries.clear();
        self.entries
            .extend(bodies.into_iter().map(|placed| GridBody {
                id: placed.id,
                key: placed.key,
                at: placed.at,
                radius: placed.radius,
                row: 0,
                column: 0,
            }));
        self.widest = self
            .entries
            .iter()
            .map(|body| body.radius)
            .max()
            .unwrap_or(Num::ZERO);
        let side = self
            .widest
            .checked_mul_int(2)
            .expect("a body's radius is bounded");
        self.cell = side.max(Num::ONE).to_bits();
        let cell = self.cell;
        for body in &mut self.entries {
            let at = body.at.get();
            body.row = at.z.to_bits().div_euclid(cell);
            body.column = at.x.to_bits().div_euclid(cell);
        }
        self.entries
            .sort_unstable_by_key(|body| (body.row, body.column, body.id));
        self.starts.clear();
        let (Some(first), Some(last)) = (self.entries.first(), self.entries.last()) else {
            return;
        };
        let (first, last) = (first.row, last.row);
        let count = self.entries.len() as u64;
        if last.abs_diff(first) >= BodyGrid::<K>::SPREAD * count + BodyGrid::<K>::ROWS {
            return;
        }
        (self.first_row, self.last_row) = (first, last);
        let rows = usize::try_from(last.abs_diff(first)).expect("rows within the spread") + 1;
        self.starts.reserve_exact(rows + 1);
        let mut at = 0;
        for row in first..=last {
            self.starts
                .push(u32::try_from(at).expect("a grid's bodies fit u32"));
            while self.entries.get(at).is_some_and(|body| body.row == row) {
                at += 1;
            }
        }
        self.starts
            .push(u32::try_from(at).expect("a grid's bodies fit u32"));
    }

    /// Calls `visit` with each body whose disc may reach into the box of the ground plane from
    /// `low` to `high`, `[x, z]` each, each body once, in no order a caller may rely on.
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
        let in_columns = |run: &[GridBody<K>]| {
            let start = run.partition_point(|body| body.column < *columns.start());
            let end = run.partition_point(|body| body.column <= *columns.end());
            start..end.max(start)
        };
        if self.starts.is_empty() {
            for row in rows {
                let start = self
                    .entries
                    .partition_point(|body| (body.row, body.column) < (row, *columns.start()));
                let run = self.entries[start..]
                    .iter()
                    .take_while(|body| body.row == row && body.column <= *columns.end());
                run.for_each(&mut visit);
            }
            return;
        }
        for row in *rows.start().max(&self.first_row)..=*rows.end().min(&self.last_row) {
            let at = usize::try_from(row - self.first_row).expect("a row within the starts");
            let run = &self.entries[self.starts[at] as usize..self.starts[at + 1] as usize];
            run[in_columns(run)].iter().for_each(&mut visit);
        }
    }

    /// Calls `visit` with each body whose disc may come within `reach` of `at` on the ground
    /// plane, as `visit` does.
    pub(crate) fn visit_near(&self, at: Position, reach: Num, visit: impl FnMut(&GridBody<K>)) {
        let at = at.get();
        let low = [at.x, at.z].map(|axis| axis.checked_sub(reach).unwrap_or(Num::MIN));
        let high = [at.x, at.z].map(|axis| axis.checked_add(reach).unwrap_or(Num::MAX));
        self.visit(low, high, visit);
    }
}

#[cfg(test)]
mod tests;
