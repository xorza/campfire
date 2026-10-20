use campfire_common::DeclaredName;
use campfire_declarations::{Section, SectionTable};
use campfire_script::{DataSection, DataTable};
use serde::{Deserialize, Serialize};

/// A unit type's `drop_off` section: the mode's player resources a worker's load of joins its
/// player's there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DropOffData {
    pub resources: Vec<DeclaredName>,
}

impl DataSection for DropOffData {
    const TABLE: DataTable = DataTable::new("A unit type's `drop_off`");
}

impl Section for DropOffData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "drop_off";
}
