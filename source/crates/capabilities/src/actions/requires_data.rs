use serde::Deserialize;

use crate::scripts::data_section::DataSection;
use crate::scripts::script_api::data_table::DataTable;
use crate::values::declared_name::DeclaredName;

/// A train's `requires`: unit types of the action's package, of each of which its player must own
/// a living, complete unit, and player modifiers of the package it must hold.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequiresData {
    #[serde(default)]
    pub units: Vec<DeclaredName>,
    #[serde(default)]
    pub modifiers: Vec<DeclaredName>,
}

impl DataSection for RequiresData {
    const TABLE: DataTable = DataTable::Requires;
}
