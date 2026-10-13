use std::num::NonZeroU8;

use campfire_core::{DataSection, DataTable};
use campfire_values::DeclaredName;
use serde::Deserialize;

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
