use campfire_common::DeclaredName;
use campfire_declarations::{Section, SectionTable};
use serde::Deserialize;

/// The mode's `resources`: its players' resources, such as gold, which share no name with a pool.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct ResourcesData(pub Vec<DeclaredName>);

impl Section for ResourcesData {
    const TABLE: SectionTable = SectionTable::Mode;
    const KEY: &'static str = "resources";
}
