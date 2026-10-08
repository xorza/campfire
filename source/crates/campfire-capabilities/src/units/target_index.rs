use campfire_math::Num;
use campfire_sim::Position;

use crate::units::body_grid::{BodyGrid, GridBody, Placed};
use crate::units::unit_row::UnitRow;

/// The bodies of the units of the script view that may be targets, by row, which a query that
/// reaches by distance indexes once after each read; and the rows such a query found.
#[derive(Debug, Default)]
pub(crate) struct TargetIndex {
    bodies: BodyGrid<usize>,
    indexed: bool,
    found: Vec<usize>,
}

impl TargetIndex {
    /// Forgets the bodies, as a read changed the rows they came from.
    pub(crate) const fn forget(&mut self) {
        self.indexed = false;
    }

    /// Indexes the bodies of the targets among `rows`, unless it did since the last read.
    pub(crate) fn index(&mut self, rows: &[UnitRow]) {
        if self.indexed {
            return;
        }
        let rows = rows.iter().enumerate();
        let targets = rows.filter(|(_, row)| row.targetable);
        self.bodies.rebuild(targets.map(|(at, row)| Placed {
            id: row.id,
            key: at,
            at: row.pos,
            shape: row.shape,
        }));
        self.indexed = true;
    }

    /// Calls `visit` with each target's body whose bounding box comes within `reach` of `at`.
    pub(crate) fn visit_near(&self, at: Position, reach: Num, visit: impl FnMut(&GridBody<usize>)) {
        debug_assert!(self.indexed, "a query indexes the bodies first");
        self.bodies.visit_near(at, reach, visit);
    }

    /// The rows of the targets whose body's bounding box comes within `reach` of `at` and that
    /// `keep` keeps, in row order.
    pub(crate) fn find_near(
        &mut self,
        at: Position,
        reach: Num,
        mut keep: impl FnMut(&GridBody<usize>) -> bool,
    ) -> &[usize] {
        debug_assert!(self.indexed, "a query indexes the bodies first");
        self.found.clear();
        self.bodies.visit_near(at, reach, |body| {
            if keep(body) {
                self.found.push(body.key);
            }
        });
        self.found.sort_unstable();
        &self.found
    }
}
