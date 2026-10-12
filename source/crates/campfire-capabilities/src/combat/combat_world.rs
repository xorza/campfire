use bevy_ecs::world::World;

use crate::stats::life_pool::LifePool;
use crate::stats::pool_id::PoolId;

/// What the combat module gives a world, for every crate.
pub trait CombatWorld {
    /// The match's life pool; `None` when its mode names none.
    fn life_pool(&self) -> Option<PoolId>;
}

impl CombatWorld for World {
    fn life_pool(&self) -> Option<PoolId> {
        Some(self.get_resource::<LifePool>()?.0)
    }
}
