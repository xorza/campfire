use campfire_script::{DataSection, DataTable};
use campfire_values::DeclaredName;
use serde::Deserialize;

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
