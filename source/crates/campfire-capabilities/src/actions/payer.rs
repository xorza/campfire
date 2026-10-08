use campfire_common::PlayerSlot;

use crate::players::player_resources::PlayerResources;
use crate::players::resource_amount::ResourceAmount;
use crate::stats::pool_cost::PoolCost;
use crate::stats::pools::Pools;
use crate::units::owner::Owner;

/// What a unit pays an action's cost with, as its `Purse` reads it: its pools, and the resources
/// of the player that owns it, when the match keeps players' resources.
#[derive(Debug)]
pub(crate) struct Payer<'a> {
    pools: Option<&'a mut Pools>,
    resources: Option<&'a mut PlayerResources>,
    owner: Option<PlayerSlot>,
}

impl<'a> Payer<'a> {
    /// The payer of a unit of `pools`, which `owner` owns, in a match that keeps `resources`.
    pub(crate) fn of(
        pools: Option<&'a mut Pools>,
        resources: Option<&'a mut PlayerResources>,
        owner: Option<&Owner>,
    ) -> Payer<'a> {
        Payer {
            pools,
            resources,
            owner: owner.copied().map(Owner::slot),
        }
    }

    /// Pays `cost` from its pools and `resources` from its player's, which its purse affords:
    /// every action's cost is paid here.
    pub(crate) fn pay(self, cost: &PoolCost, resources: &[ResourceAmount]) {
        if let Some(pools) = self.pools {
            pools.pay(cost);
        }
        if let (Some(owner), Some(amounts)) = (self.owner, self.resources) {
            amounts.pay(owner, resources);
        }
    }
}
