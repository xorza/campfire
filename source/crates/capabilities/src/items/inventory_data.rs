use std::num::NonZeroU8;

use serde::Deserialize;

use crate::scripts::data_section::DataSection;
use crate::scripts::script_api::data_table::DataTable;
use crate::values::declared_name::DeclaredName;

/// A unit type's `inventory` section: how many slots it carries items in, at least one, and the
/// mode's slot kind whose action slots they fill, one each, in order.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InventoryData {
    pub slots: NonZeroU8,
    pub kind: DeclaredName,
}

impl DataSection for InventoryData {
    const TABLE: DataTable = DataTable::Inventory;
}
