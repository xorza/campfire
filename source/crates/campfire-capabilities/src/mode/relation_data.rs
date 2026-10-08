use serde::Deserialize;

use crate::values::declared_name::DeclaredName;
use crate::values::relation::Relation;

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
