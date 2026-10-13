use crate::players::player_resources::PlayerResources;
use crate::players::resource_amount::ResourceAmount;
use crate::stats::pool_cost::PoolCost;
use crate::stats::unit_pools::PoolMeters;

/// What a unit pays an action's cost from: its pools, and the resources of the player that owns
/// it, when the match keeps players' resources.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Purse<'a> {
    pools: PoolMeters<'a>,
    resources: Option<&'a PlayerResources>,
}

impl<'a> Purse<'a> {
    /// The purse of a unit of `pools`, whose player holds `resources`: none for a unit no player
    /// owns, or in a match that keeps no players' resources.
    pub(crate) const fn of(
        pools: PoolMeters<'a>,
        resources: Option<&'a PlayerResources>,
    ) -> Purse<'a> {
        Purse { pools, resources }
    }

    /// Whether it holds `cost` in its pools and `resources` in its player's: a unit affords no
    /// cost in a pool it lacks, and one no player owns none in resources.
    pub(crate) fn affords(&self, cost: &PoolCost, resources: &[ResourceAmount]) -> bool {
        let pools = self.pools.affords(cost);
        let player = self.resources.map_or(resources.is_empty(), |held| {
            resources
                .iter()
                .all(|cost| held.amount(cost.resource) >= cost.amount)
        });
        pools && player
    }
}
