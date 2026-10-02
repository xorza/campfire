use crate::players::resource_id::ResourceId;

/// An amount of one player resource, as an action's cost names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceAmount {
    pub resource: ResourceId,
    pub amount: i64,
}
