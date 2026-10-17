use campfire_players::{PlayerResources, ResourceAmount};
use campfire_units::{PoolCost, PoolPurse};

/// What a unit pays an action's cost from: its pools, and the resources of the player that owns
/// it, when the match keeps players' resources.
#[derive(Debug, Clone, Copy)]
pub struct Purse<'a, P> {
    pools: P,
    resources: Option<&'a PlayerResources>,
}

impl<'a, P: PoolPurse> Purse<'a, P> {
    /// The purse of a unit of `pools`, whose player holds `resources`: none for a unit no player
    /// owns, or in a match that keeps no players' resources.
    pub const fn of(pools: P, resources: Option<&'a PlayerResources>) -> Self {
        Purse { pools, resources }
    }

    /// Whether it holds `cost` in its pools and `resources` in its player's: a unit affords no
    /// cost in a pool it lacks, and one no player owns none in resources.
    pub fn affords(&self, cost: &PoolCost, resources: &[ResourceAmount]) -> bool {
        let pools = self.pools.affords(cost);
        let player = self
            .resources
            .map_or(resources.is_empty(), |held| held.affords(resources));
        pools && player
    }
}
