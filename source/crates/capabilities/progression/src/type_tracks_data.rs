use campfire_common::DeclaredName;
use campfire_declare::{Section, SectionTable};
use serde::Deserialize;

/// A unit type's `tracks`: the mode's tracks it gains experience on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct TypeTracksData(pub Vec<DeclaredName>);

impl Section for TypeTracksData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "tracks";
}
