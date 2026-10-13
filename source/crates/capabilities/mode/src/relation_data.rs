use campfire_core::{DataSection, DataTable};
use campfire_values::{DeclaredName, Relation};
use serde::Deserialize;

/// A pair of the mode's teams as its `[[relations]]` declare them: how they regard each other,
/// and, friendly, whether they share vision.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationData {
    pub teams: [DeclaredName; 2],
    pub relation: Relation,
    /// Absent: on.
    #[serde(default = "RelationData::vision_on")]
    pub vision: bool,
}

impl RelationData {
    const fn vision_on() -> bool {
        true
    }
}

impl DataSection for RelationData {
    const TABLE: DataTable = DataTable::Relation;
}
