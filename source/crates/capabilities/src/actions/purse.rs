use campfire_math::PlayerSlot;

use crate::players::player_resources::PlayerResources;
use crate::players::resource_id::ResourceAmount;
use crate::stats::pool_cost::PoolCost;
use crate::stats::pools::Pools;

/// What a unit pays an action's cost from: its pools, and the resources of the player that owns
/// it, when the match keeps players' resources.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Purse<'a> {
    pub(crate) pools: Option<&'a Pools>,
    pub(crate) resources: Option<&'a PlayerResources>,
    pub(crate) owner: Option<PlayerSlot>,
}

impl Purse<'_> {
    /// Whether it holds `cost` in its pools and `resources` in its player's: a unit with no pool
    /// affords no cost in pools, and one no player owns none in resources.
    pub(crate) fn affords(&self, cost: &PoolCost, resources: &[ResourceAmount]) -> bool {
        let pools = self
            .pools
            .map_or_else(|| *cost == PoolCost::default(), |pools| pools.affords(cost));
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

/// What a unit pays an action's cost with, as its `Purse` reads it: its pools, and the resources
/// of the player that owns it, when the match keeps players' resources.
#[derive(Debug)]
pub(crate) struct Payer<'a> {
    pub(crate) pools: Option<&'a mut Pools>,
    pub(crate) resources: Option<&'a mut PlayerResources>,
    pub(crate) owner: Option<PlayerSlot>,
}

impl Payer<'_> {
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
