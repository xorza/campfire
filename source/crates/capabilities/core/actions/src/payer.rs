use campfire_players::{PlayerResources, ResourceAmount};
use campfire_units::{PoolCost, PoolPayer};

/// What a unit pays an action's cost with, as its `Purse` reads it: its pools, and the resources
/// of the player that owns it, when the match keeps players' resources.
#[derive(Debug)]
pub struct Payer<'a, P> {
    pools: P,
    resources: Option<&'a mut PlayerResources>,
}

impl<'a, P: PoolPayer> Payer<'a, P> {
    /// The payer of a unit of `pools`, whose player holds `resources`: none for a unit no player
    /// owns, or in a match that keeps no players' resources.
    pub const fn of(pools: P, resources: Option<&'a mut PlayerResources>) -> Self {
        Payer { pools, resources }
    }

    /// Pays `cost` from its pools and `resources` from its player's, which its purse affords: an
    /// attack's cost and a train's. A cast pays its parts apart, its resources before its call
    /// runs and its pools as it applies, and a toggle its pools alone.
    pub fn pay(mut self, cost: &PoolCost, resources: &[ResourceAmount]) {
        self.pools.pay(cost);
        if let Some(held) = self.resources {
            held.pay(resources);
        }
    }
}
