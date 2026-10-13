use campfire_core::{PlayerResources, ResourceAmount};
use campfire_stats::{PoolCost, PoolMetersMut};

/// What a unit pays an action's cost with, as its `Purse` reads it: its pools, and the resources
/// of the player that owns it, when the match keeps players' resources.
#[derive(Debug)]
pub struct Payer<'a> {
    pools: PoolMetersMut<'a>,
    resources: Option<&'a mut PlayerResources>,
}

impl<'a> Payer<'a> {
    /// The payer of a unit of `pools`, whose player holds `resources`: none for a unit no player
    /// owns, or in a match that keeps no players' resources.
    pub const fn of(
        pools: PoolMetersMut<'a>,
        resources: Option<&'a mut PlayerResources>,
    ) -> Payer<'a> {
        Payer { pools, resources }
    }

    /// Pays `cost` from its pools and `resources` from its player's, which its purse affords:
    /// every action's cost is paid here.
    pub fn pay(mut self, cost: &PoolCost, resources: &[ResourceAmount]) {
        self.pools.pay(cost);
        if let Some(held) = self.resources {
            held.pay(resources);
        }
    }
}
