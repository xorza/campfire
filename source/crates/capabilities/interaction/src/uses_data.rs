use campfire_common::DeclaredName;
use campfire_declarations::{Section, SectionTable};
use campfire_script::{DataSection, DataTable};
use serde::Deserialize;

/// A unit type's `uses` section, which lets its units use objects: the mode's slot kind of the one
/// action slot that holds the use under way.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsesData {
    pub kind: DeclaredName,
}

impl DataSection for UsesData {
    const TABLE: DataTable = DataTable::new("A unit type's `uses`");
}

impl Section for UsesData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "uses";
}
