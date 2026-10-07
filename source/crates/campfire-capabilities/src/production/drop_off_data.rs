use serde::Deserialize;

use crate::values::declared_name::DeclaredName;

/// A unit type's `drop_off` section: the mode's player resources a worker's load of joins its
/// player's there.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DropOffData {
    pub resources: Vec<DeclaredName>,
}
