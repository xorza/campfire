use bevy_ecs::world::World;

use crate::stats::pool_book::PoolBook;
use crate::stats::pool_id::PoolId;

/// What the combat module gives a world, for every crate.
pub trait CombatWorld {
    /// The match's life pool; `None` when its mode names none.
    fn life_pool(&self) -> Option<PoolId>;
}

impl CombatWorld for World {
    fn life_pool(&self) -> Option<PoolId> {
        self.get_resource::<PoolBook>()?.life()
    }
}
