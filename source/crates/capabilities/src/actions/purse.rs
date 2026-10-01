use campfire_math::PlayerSlot;

use crate::mode::player_resources::PlayerResources;
use crate::mode::resource_id::ResourceAmount;
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
