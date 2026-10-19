use campfire_declare::{Section, SectionTable};
use std::num::NonZeroU8;

use campfire_common::DeclaredName;
use campfire_script::{DataSection, DataTable};
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
    const TABLE: DataTable = DataTable::new("A unit type's `inventory`");
}

impl Section for InventoryData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "inventory";
}
