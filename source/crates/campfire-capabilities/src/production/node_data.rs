use std::num::NonZeroU32;

use serde::Deserialize;

use crate::values::declared_name::DeclaredName;

/// A unit type's `node` section: a node of the mode's player resource `resource`, holding
/// `amount` as it spawns.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeData {
    pub resource: DeclaredName,
    pub amount: NonZeroU32,
}
