use campfire_common::PlayerSlot;

use crate::players::player_resources::PlayerResources;
use crate::players::resource_amount::ResourceAmount;
use crate::stats::pool_cost::PoolCost;
use crate::stats::unit_pools::PoolMeters;
use crate::units::owner::Owner;

/// What a unit pays an action's cost from: its pools, and the resources of the player that owns
/// it, when the match keeps players' resources.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Purse<'a> {
    pools: PoolMeters<'a>,
    resources: Option<&'a PlayerResources>,
    owner: Option<PlayerSlot>,
}

impl<'a> Purse<'a> {
    /// The purse of a unit of `pools`, which `owner` owns, in a match that keeps `resources`.
    pub(crate) fn of(
        pools: PoolMeters<'a>,
        resources: Option<&'a PlayerResources>,
        owner: Option<&Owner>,
    ) -> Purse<'a> {
        Purse {
            pools,
            resources,
            owner: owner.copied().map(Owner::slot),
        }
    }

    /// Whether it holds `cost` in its pools and `resources` in its player's: a unit affords no
    /// cost in a pool it lacks, and one no player owns none in resources.
    pub(crate) fn affords(&self, cost: &PoolCost, resources: &[ResourceAmount]) -> bool {
        let pools = self.pools.affords(cost);
        let held = |owner, amounts: &PlayerResources| {
            resources
                .iter()
                .all(|cost| amounts.amount(owner, cost.resource) >= cost.amount)
        };
        let player = match (self.owner, self.resources) {
            (Some(owner), Some(amounts)) => held(owner, amounts),
            _ => resources.is_empty(),
        };
        pools && player
    }
}
