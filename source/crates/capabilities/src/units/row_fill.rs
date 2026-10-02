use std::fmt;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::{QueryState, ROQueryItem, ReadOnlyQueryData};
use bevy_ecs::world::World;

use crate::units::unit_row::UnitRow;
use crate::units::view_column::{ViewColumn, ViewColumns};

/// How a capability above the core fills its fields of a unit's row: from the parts `D` of the
/// unit, through a query the view keeps from one read to the next, so a read looks up no part by
/// its type.
pub(crate) struct RowSource<D: ReadOnlyQueryData + 'static> {
    parts: QueryState<D>,
    fill: for<'w, 's> fn(ROQueryItem<'w, 's, D>, &mut RowFill<'_>),
}

impl<D: ReadOnlyQueryData + 'static> RowSource<D> {
    /// A source of `world` that fills a row by `fill` from the unit's parts `D`.
    pub(crate) fn new(
        world: &mut World,
        fill: for<'w, 's> fn(ROQueryItem<'w, 's, D>, &mut RowFill<'_>),
    ) -> RowSource<D> {
        RowSource {
            parts: QueryState::new(world),
            fill,
        }
    }
}

/// A query prints only what it reads.
impl<D: ReadOnlyQueryData + 'static> fmt::Debug for RowSource<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RowSource")
            .field("parts", &self.parts)
            .finish_non_exhaustive()
    }
}

/// A row source of any parts, as the view holds them.
pub(crate) trait FillRow: fmt::Debug {
    /// Brings its query up to the archetypes of `world`, once before a read.
    fn update(&mut self, world: &World);

    /// Fills `fill`, the row of `entity` of `world`.
    fn fill(&self, world: &World, entity: Entity, fill: &mut RowFill<'_>);
}

impl<D: ReadOnlyQueryData + 'static> FillRow for RowSource<D> {
    fn update(&mut self, world: &World) {
        self.parts.update_archetypes(world);
    }

    fn fill(&self, world: &World, entity: Entity, fill: &mut RowFill<'_>) {
        let parts = self
            .parts
            .get_manual(world, entity)
            .expect("a source reads optional parts of any unit");
        (self.fill)(parts, fill);
    }
}

/// A row the view reads, as a capability fills it: its fields, and the columns.
#[derive(Debug)]
pub(crate) struct RowFill<'a> {
    pub(crate) row: &'a mut UnitRow,
    pub(crate) world: &'a World,
    pub(super) columns: &'a mut ViewColumns,
}

impl RowFill<'_> {
    /// The column of type `C`, which the source's capability added, for it to add the row's
    /// part to.
    pub(crate) fn column<C: ViewColumn>(&mut self) -> &mut C {
        self.columns
            .get_mut()
            .expect("a source fills the column its capability added")
    }
}
