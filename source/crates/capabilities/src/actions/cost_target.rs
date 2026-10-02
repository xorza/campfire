use crate::players::resource_id::ResourceId;
use crate::stats::pool_id::PoolId;

/// What a name of a cost takes from: a pool of the unit, or a resource of its player; the mode's
/// pools and player resources never share a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CostTarget {
    Pool(PoolId),
    Resource(ResourceId),
}
