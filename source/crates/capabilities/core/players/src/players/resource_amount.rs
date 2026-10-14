use campfire_values::ResourceId;
use serde::{Deserialize, Serialize};

/// An amount of one player resource, as an action's cost names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceAmount {
    pub resource: ResourceId,
    pub amount: i64,
}
