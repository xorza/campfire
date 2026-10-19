use campfire_common::DeclaredName;
use campfire_script::{DataSection, DataTable};
use serde::{Deserialize, Serialize};

/// A train's or a build's `requires`: unit types of the action's package, of each of which its
/// player must own a living, complete unit, and player modifiers of the package it must hold.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequiresData {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub units: Vec<DeclaredName>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modifiers: Vec<DeclaredName>,
}

impl DataSection for RequiresData {
    const TABLE: DataTable = DataTable::new("A train's `requires`");
}
