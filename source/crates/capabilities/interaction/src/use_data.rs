use campfire_common::DeclaredName;
use campfire_declare::{Section, SectionTable};
use campfire_script::{DataSection, DataTable};
use campfire_values::FilterData;
use serde::Deserialize;

/// A unit type's `use` section, which makes its units objects others use: an action of its
/// package of the `use` kind, which a user's use slot runs at it, and the units that may use it,
/// by their relation to it and their tags, all of them when absent.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UseData {
    pub action: DeclaredName,
    #[serde(default = "FilterData::all")]
    pub users: FilterData,
}

impl DataSection for UseData {
    const TABLE: DataTable = DataTable::Use;
}

impl Section for UseData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "use";
}
