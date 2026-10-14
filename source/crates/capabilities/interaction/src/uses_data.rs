use campfire_script::{DataSection, DataTable};
use campfire_values::DeclaredName;
use serde::Deserialize;

/// A unit type's `uses` section, which lets its units use objects: the mode's slot kind of the one
/// action slot that holds the use under way.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsesData {
    pub kind: DeclaredName,
}

impl DataSection for UsesData {
    const TABLE: DataTable = DataTable::Uses;
}
