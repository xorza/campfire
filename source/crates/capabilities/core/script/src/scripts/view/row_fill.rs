use campfire_values::Relations;

use crate::scripts::view::unit_row::UnitRow;

/// A row the view reads, as a capability fills it: the core row, whose fields of the capability
/// it sets, its column, to add the row to, and how the teams regard each other.
#[derive(Debug)]
pub struct RowFill<'a, C> {
    pub row: &'a mut UnitRow,
    pub column: &'a mut C,
    pub relations: &'a Relations,
}
