use bevy_ecs::entity::Entity;
use bevy_ecs::world::{Mut, World};
use campfire_common::PlayerSlot;
use campfire_players::PlayersWorld;
use campfire_units::{Dead, Life, PoolId, lives};

use crate::player_modifiers::PlayerModifiers;
use crate::pool_book::PoolBook;

/// What the stats module gives a world, for every crate.
pub trait StatsWorld {
    /// The match's life pool; `None` when its mode names none.
    fn life_pool(&self) -> Option<PoolId>;

    /// The modifiers player `slot` holds, to change; none before its entity spawns.
    fn player_modifiers_mut(&mut self, slot: PlayerSlot) -> Option<Mut<'_, PlayerModifiers>>;

    /// Whether the unit of `entity` lives, as `lives` says.
    fn lives(&self, entity: Entity) -> bool;
}

impl StatsWorld for World {
    fn life_pool(&self) -> Option<PoolId> {
        self.get_resource::<PoolBook>()?.life()
    }

    fn player_modifiers_mut(&mut self, slot: PlayerSlot) -> Option<Mut<'_, PlayerModifiers>> {
        let entity = self.player_entity(slot)?;
        self.get_mut(entity)
    }

    fn lives(&self, entity: Entity) -> bool {
        lives(self.get::<Dead>(entity).is_some(), self.get::<Life>(entity))
    }
}
