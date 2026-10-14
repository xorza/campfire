use campfire_geometry::{BodyGrid, Placed, Shape};
use campfire_math::Num;
use campfire_sim::Position;
use campfire_values::Team;

use crate::scripts::view::filter::Filter;
use crate::scripts::view::unit_row::UnitRow;
use crate::scripts::view::view_column::ViewColumns;
use crate::scripts::view::view_rows::ViewRows;

/// The bodies of the units of the script view that may be targets, by row, which a query that
/// reaches by distance indexes once after each read; and the rows such a query found.
#[derive(Debug, Default)]
pub(crate) struct TargetIndex {
    bodies: BodyGrid<usize>,
    indexed: bool,
    found: Vec<usize>,
}

/// What a query of the targets asks: those whose bodies `radius`, not negative, from the edge of
/// a body of `shape` at `from` reaches in the map's metric, and that `filter` selects relative to
/// `team`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TargetQuery<'a> {
    pub team: Team,
    pub(crate) from: Position,
    pub shape: Shape,
    pub(crate) radius: Num,
    pub(crate) filter: &'a Filter,
}

impl TargetIndex {
    /// Forgets the bodies, as a read changed the rows they came from.
    pub(crate) const fn forget(&mut self) {
        self.indexed = false;
    }

    /// Indexes the bodies of the targets among `rows`, unless it did since the last read.
    fn index(&mut self, rows: &[UnitRow]) {
        if self.indexed {
            return;
        }
        let rows = rows.iter().enumerate();
        let targets = rows.filter(|(_, row)| row.targetable);
        self.bodies.rebuild(targets.map(|(at, row)| Placed {
            id: row.id,
            key: at,
            pos: row.pos,
            shape: row.shape,
        }));
        self.indexed = true;
    }

    /// Calls `visit` with the body of each target of `rows` that `query` reaches and `seen` lets
    /// by the rows' columns, in the order the grid holds them.
    pub(crate) fn visit(
        &mut self,
        rows: &ViewRows,
        query: TargetQuery<'_>,
        seen: impl Fn(&ViewColumns, usize) -> bool,
        visit: impl FnMut(&Placed<usize>),
    ) {
        self.index(rows.units());
        TargetIndex::each_reached(&self.bodies, rows, query, seen, visit);
    }

    /// The rows of the targets of `rows` that `query` reaches and `seen` lets, in row order.
    pub(crate) fn find(
        &mut self,
        rows: &ViewRows,
        query: TargetQuery<'_>,
        seen: impl Fn(&ViewColumns, usize) -> bool,
    ) -> &[usize] {
        self.index(rows.units());
        self.found.clear();
        TargetIndex::each_reached(&self.bodies, rows, query, seen, |body| {
            self.found.push(body.key);
        });
        self.found.sort_unstable();
        &self.found
    }

    /// Calls `visit` with each body of `bodies` that `query` reaches and `seen` lets.
    fn each_reached(
        bodies: &BodyGrid<usize>,
        rows: &ViewRows,
        query: TargetQuery<'_>,
        seen: impl Fn(&ViewColumns, usize) -> bool,
        mut visit: impl FnMut(&Placed<usize>),
    ) {
        let TargetQuery {
            team,
            from,
            shape,
            radius,
            filter,
        } = query;
        let reach = shape.bound().checked_add(radius).unwrap_or(Num::MAX);
        let metric = rows.metric();
        bodies.visit_near(from, reach, |body| {
            let row = &rows.units()[body.key];
            let relation = rows.relations().between(team, row.team);
            if metric.reaches(from, shape, radius, body.pos, body.shape)
                && filter.selects(relation, row.tags.tags)
                && seen(rows.columns(), body.key)
            {
                visit(body);
            }
        });
    }
}
