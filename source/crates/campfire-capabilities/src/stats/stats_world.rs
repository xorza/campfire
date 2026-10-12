use bevy_ecs::world::{Mut, World};
use campfire_common::PlayerSlot;

use crate::players::players_world::PlayersWorld;
use crate::stats::player_modifiers::PlayerModifiers;

/// What the stats module gives a world, for its own crate.
pub(crate) trait StatsWorldCrate {
    /// The modifiers player `slot` holds, to change; none before its entity spawns.
    fn player_modifiers_mut(&mut self, slot: PlayerSlot) -> Option<Mut<'_, PlayerModifiers>>;
}

impl StatsWorldCrate for World {
    fn player_modifiers_mut(&mut self, slot: PlayerSlot) -> Option<Mut<'_, PlayerModifiers>> {
        let entity = self.player_entity(slot)?;
        self.get_mut(entity)
    }
}
