use crate::players::resource_id::ResourceId;
use crate::stats::pool_id::PoolId;
use crate::values::names::Names;

/// What a name of a cost takes from: a pool of the unit, or a resource of its player; the mode's
/// pools and player resources never share a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CostTarget {
    Pool(PoolId),
    Resource(ResourceId),
}

impl CostTarget {
    /// What a cost named `name` takes from: one of `pools`, or else one of `resources`; `None`
    /// for a name neither holds.
    pub fn named(
        pools: &Names<PoolId>,
        resources: &Names<ResourceId>,
        name: &str,
    ) -> Option<CostTarget> {
        let pool = pools.named(name).map(CostTarget::Pool);
        pool.or_else(|| resources.named(name).map(CostTarget::Resource))
    }
}
