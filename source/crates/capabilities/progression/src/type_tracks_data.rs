use campfire_common::DeclaredName;
use campfire_declarations::{Section, SectionTable};
use campfire_script::{ApiVersion, Status};
use serde::Deserialize;

/// A unit type's `tracks`: the mode's tracks it gains experience on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct TypeTracksData(pub Vec<DeclaredName>);

impl Section for TypeTracksData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "tracks";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}
