use crate::pool_cost::PoolCost;

/// What an action's cost in pools is read from, as the capability that keeps a unit's pools
/// gives it, so the action pipeline names no keeper of pools: whether the unit holds `cost`. A
/// unit affords no cost in a pool it lacks.
pub trait PoolPurse {
    fn affords(&self, cost: &PoolCost) -> bool;
}

/// What an action's cost in pools is paid from, as `PoolPurse` reads it.
pub trait PoolPayer: PoolPurse {
    /// Spends `cost`, which it affords.
    fn pay(&mut self, cost: &PoolCost);
}
