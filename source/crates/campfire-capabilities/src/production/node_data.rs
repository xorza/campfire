use std::num::NonZeroU32;

use serde::Deserialize;

use crate::scripts::data_section::DataSection;
use crate::scripts::script_api::data_table::DataTable;
use crate::values::declared_name::DeclaredName;

/// A unit type's `node` section: a node of the mode's player resource `resource`, holding
/// `amount` as it spawns.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeData {
    pub resource: DeclaredName,
    pub amount: NonZeroU32,
}

impl DataSection for NodeData {
    const TABLE: DataTable = DataTable::Node;
}
