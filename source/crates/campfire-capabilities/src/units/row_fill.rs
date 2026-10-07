use std::fmt;
use std::marker::PhantomData;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::{QueryState, ROQueryItem};
use bevy_ecs::system::{Query, SystemState};
use bevy_ecs::world::World;

use crate::units::relations::Relations;
use crate::units::row_marks::RowMarks;
use crate::units::row_parts::RowParts;
use crate::units::unit_row::UnitRow;
use crate::units::view_column::{ViewColumn, ViewColumns};

/// How a capability above the core fills its column of a unit's row, and the fields of the core
/// row it owns: from the parts `D` of the unit, through queries the view keeps from one read to
/// the next, so a read looks up no part by its type. It fills only the rows of units whose parts
/// changed since its last read; the column keeps the others.
pub(crate) struct RowSource<D: RowParts, C: ViewColumn> {
    parts: QueryState<D>,
    /// The units whose parts changed since the source's last read.
    changed: SystemState<Query<'static, 'static, Entity, D::Changed>>,
    /// The place of its column among the view's.
    column: usize,
    fill: for<'w, 's> fn(ROQueryItem<'w, 's, D>, &mut RowFill<'_, C>),
    of: PhantomData<C>,
}

impl<D: RowParts, C: ViewColumn> RowSource<D, C> {
    /// A source of `world` that fills the row of its column at `column` by `fill` from the unit's
    /// parts `D`.
    pub(crate) fn new(
        world: &mut World,
        column: usize,
        fill: for<'w, 's> fn(ROQueryItem<'w, 's, D>, &mut RowFill<'_, C>),
    ) -> RowSource<D, C> {
        RowSource {
            parts: QueryState::new(world),
            changed: SystemState::new(world),
            column,
            fill,
            of: PhantomData,
        }
    }
}

/// A query prints only what it reads.
impl<D: RowParts, C: ViewColumn> fmt::Debug for RowSource<D, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RowSource")
            .field("parts", &self.parts)
            .field("column", &self.column)
            .finish_non_exhaustive()
    }
}

/// A row source of any parts, as the view holds them.
pub(crate) trait FillRow: fmt::Debug {
    /// Starts a read of `world`: brings its queries up to the archetypes, starts its column's
    /// read, and marks for `source` each unit whose parts changed since its last read. Whether
    /// every row must be filled again, as the column asks.
    fn begin(
        &mut self,
        world: &World,
        columns: &mut ViewColumns,
        source: usize,
        marks: &mut RowMarks,
    ) -> bool;

    /// Fills the row of `entity` of `world` into `row` and its column.
    fn fill(
        &self,
        world: &World,
        entity: Entity,
        row: &mut UnitRow,
        columns: &mut ViewColumns,
        relations: &Relations,
    );

    /// Adds row `row` of the read before to its column, unchanged.
    fn keep(&self, columns: &mut ViewColumns, row: usize);
}

impl<D: RowParts, C: ViewColumn> FillRow for RowSource<D, C> {
    fn begin(
        &mut self,
        world: &World,
        columns: &mut ViewColumns,
        source: usize,
        marks: &mut RowMarks,
    ) -> bool {
        self.parts.update_archetypes(world);
        let changed = self.changed.get(world).expect("a query is always valid");
        for entity in &changed {
            marks.mark(entity, source);
        }
        columns.any_mut(self.column).begin(world)
    }

    fn fill(
        &self,
        world: &World,
        entity: Entity,
        row: &mut UnitRow,
        columns: &mut ViewColumns,
        relations: &Relations,
    ) {
        let parts = self
            .parts
            .get_manual(world, entity)
            .expect("a source reads optional parts of any unit");
        let mut fill = RowFill {
            row,
            column: columns.at_mut(self.column),
            relations,
        };
        (self.fill)(parts, &mut fill);
    }

    fn keep(&self, columns: &mut ViewColumns, row: usize) {
        columns.any_mut(self.column).keep(row);
    }
}

/// A row the view reads, as a capability fills it: the core row, whose fields of the capability
/// it sets, its column, to add the row to, and how the teams regard each other.
#[derive(Debug)]
pub(crate) struct RowFill<'a, C> {
    pub(crate) row: &'a mut UnitRow,
    pub(crate) column: &'a mut C,
    pub(crate) relations: &'a Relations,
}
