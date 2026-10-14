use bevy_ecs::world::{Mut, World};
use campfire_common::PlayerSlot;
use campfire_players::PlayersWorld;

use crate::player_modifiers::PlayerModifiers;
use crate::pool_book::PoolBook;
use crate::pool_id::PoolId;

/// What the stats module gives a world, for every crate.
pub trait StatsWorld {
    /// The match's life pool; `None` when its mode names none.
    fn life_pool(&self) -> Option<PoolId>;
}

impl StatsWorld for World {
    fn life_pool(&self) -> Option<PoolId> {
        self.get_resource::<PoolBook>()?.life()
    }
}

/// What the stats module gives a world, for its own crate.
pub trait StatsWorldCrate {
    /// The modifiers player `slot` holds, to change; none before its entity spawns.
    fn player_modifiers_mut(&mut self, slot: PlayerSlot) -> Option<Mut<'_, PlayerModifiers>>;
}

impl StatsWorldCrate for World {
    fn player_modifiers_mut(&mut self, slot: PlayerSlot) -> Option<Mut<'_, PlayerModifiers>> {
        let entity = self.player_entity(slot)?;
        self.get_mut(entity)
    }
}
