use serde::Deserialize;

use crate::scripts::data_section::DataSection;
use crate::scripts::script_api::data_table::DataTable;
use crate::values::package_path::PackagePath;

/// A unit type's AI as its `orders` section declares it: the script whose `on_think` runs, and how
/// often, in milliseconds.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiData {
    pub ai: PackagePath,
    pub think_ms: u64,
}

impl DataSection for AiData {
    const TABLE: DataTable = DataTable::Ai;
}
