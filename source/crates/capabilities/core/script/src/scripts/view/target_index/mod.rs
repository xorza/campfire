use std::ops::ControlFlow;

use campfire_geometry::{BodyGrid, Indexed, Placed, Shape};
use campfire_math::{Num, Vec3};
use campfire_sim::StableId;
use campfire_values::{TagSet, Team};

use crate::script_host::call_charge::CallCharge;
use crate::scripts::view::filter::Filter;
use crate::scripts::view::unit_row::UnitRow;
use crate::scripts::view::view_column::ViewColumns;
use crate::scripts::view::view_rows::ViewRows;

/// The bodies of the units of the script view that may be targets, by row, which a query that
/// reaches by distance indexes once after each read; and the rows such a query found. Each class
/// of targets, a team and a tag set, has a grid of its own, as a filter reads only those, so a
/// query walks only the classes its filter selects (06-script-api.md, Targets are indexed by
/// class).
#[derive(Debug, Default)]
pub(crate) struct TargetIndex {
    /// By team, then tags; kept between reads, a class with no target cleared, so a read
    /// allocates nothing once the match has met its classes.
    classes: Vec<TargetClass>,
    indexed: bool,
    found: Vec<usize>,
}

/// The targets of one team and one tag set.
#[derive(Debug)]
struct TargetClass {
    team: Team,
    tags: TagSet,
    /// The read's targets of the class, which its grid indexes.
    entries: Vec<Indexed<usize, StableId>>,
    bodies: BodyGrid<usize, StableId>,
}

/// What a query of the targets asks: those whose bodies `radius`, not negative, from the edge of
/// a body of `shape` at `from` reaches in the map's metric, and that `filter` selects relative to
/// `team`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TargetQuery<'a> {
    pub(crate) team: Team,
    pub(crate) from: Vec3,
    pub(crate) shape: Shape,
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
        for class in &mut self.classes {
            class.entries.clear();
        }
        for (at, row) in rows.iter().enumerate().filter(|(_, row)| row.targetable) {
            let key = (row.team, row.tags.tags);
            let class = match self
                .classes
                .binary_search_by_key(&key, |class| (class.team, class.tags))
            {
                Ok(class) => class,
                Err(class) => {
                    self.classes.insert(
                        class,
                        TargetClass {
                            team: row.team,
                            tags: row.tags.tags,
                            entries: Vec::new(),
                            bodies: BodyGrid::default(),
                        },
                    );
                    class
                }
            };
            self.classes[class].entries.push(Indexed {
                id: row.id,
                key: at,
                body: Placed::new(row.position, row.shape),
            });
        }
        for class in &mut self.classes {
            class.bodies.refresh(class.entries.iter().copied());
        }
        self.indexed = true;
    }

    /// Calls `visit` with the body of each target of `rows` that `query` reaches and `seen` lets
    /// by the rows' columns, class by class, each in the order its grid holds them, which no
    /// caller's result depends on; a step of `charge` for each body the grids give.
    pub(crate) fn visit(
        &mut self,
        rows: &ViewRows,
        query: TargetQuery<'_>,
        (seen, charge): (impl Fn(&ViewColumns, usize) -> bool, &CallCharge),
        visit: impl FnMut(&Indexed<usize, StableId>),
    ) {
        self.index(rows.units());
        Self::each_reached(&self.classes, rows, query, (seen, charge), visit);
    }

    /// The rows of the targets of `rows` that `query` reaches and `seen` lets, in row order; a
    /// step of `charge` for each body the grids give.
    pub(crate) fn find(
        &mut self,
        rows: &ViewRows,
        query: TargetQuery<'_>,
        (seen, charge): (impl Fn(&ViewColumns, usize) -> bool, &CallCharge),
    ) -> &[usize] {
        self.index(rows.units());
        self.found.clear();
        Self::each_reached(&self.classes, rows, query, (seen, charge), |entry| {
            self.found.push(entry.key);
        });
        self.found.sort_unstable();
        &self.found
    }

    /// Calls `visit` with each body of `classes` that `query` reaches and `seen` lets, walking
    /// only the classes its filter selects, a step of `charge` for each body their grids give
    /// (02-engine-core.md, A list costs a step a row).
    fn each_reached(
        classes: &[TargetClass],
        rows: &ViewRows,
        query: TargetQuery<'_>,
        (seen, charge): (impl Fn(&ViewColumns, usize) -> bool, &CallCharge),
        mut visit: impl FnMut(&Indexed<usize, StableId>),
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
        let from = Placed::new(from, shape);
        for class in classes {
            if !filter.selects(rows.relations().between(team, class.team), class.tags) {
                continue;
            }
            class.bodies.visit_near(from.position, reach, |entry| {
                charge.add(1);
                if metric.reaches(from, radius, entry.body) && seen(rows.columns(), entry.key) {
                    visit(entry);
                }
                ControlFlow::Continue(())
            });
        }
    }
}

#[cfg(test)]
mod tests;
