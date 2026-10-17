use campfire_common::DeclaredName;
use campfire_declare::{Section, SectionTable};
use campfire_script::{DataSection, DataTable};
use serde::Deserialize;

/// A unit type's `drop_off` section: the mode's player resources a worker's load of joins its
/// player's there.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DropOffData {
    pub resources: Vec<DeclaredName>,
}

impl DataSection for DropOffData {
    const TABLE: DataTable = DataTable::DropOff;
}

impl Section for DropOffData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "drop_off";
}
