use campfire_declarations::{Section, SectionTable};
use std::num::NonZeroU32;

use campfire_common::DeclaredName;
use campfire_script::{DataSection, DataTable};
use serde::Deserialize;

/// A unit type's `node` section: a node of the mode's player resource `resource`, holding
/// `amount` as it spawns.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeData {
    pub resource: DeclaredName,
    pub amount: NonZeroU32,
}

impl DataSection for NodeData {
    const TABLE: DataTable = DataTable::new("A unit type's `node`");
}

impl Section for NodeData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "node";
}
