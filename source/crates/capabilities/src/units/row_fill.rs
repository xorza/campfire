use bevy_ecs::world::{EntityRef, World};

use crate::units::unit_row::UnitRow;
use crate::units::view_column::{ViewColumn, ViewColumns};

/// Fills the fields of a unit's row that a capability above the core holds.
pub(crate) type RowSource = fn(&EntityRef<'_>, &mut RowFill<'_>);

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
