use campfire_common::DeclaredName;
use campfire_declare::{Section, SectionTable};
use serde::Deserialize;

/// A unit type's `pools`: the mode's pools its units have, by name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct TypePoolsData(pub Vec<DeclaredName>);

impl Section for TypePoolsData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "pools";
}
