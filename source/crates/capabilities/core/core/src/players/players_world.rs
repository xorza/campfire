use bevy_ecs::entity::Entity;
use bevy_ecs::world::{Mut, World};
use campfire_common::PlayerSlot;
use campfire_sim::{SimWorld, StableId};

use crate::players::player_resources::PlayerResources;
use crate::units::owner::Owner;
use crate::units::view::View;

/// What the players module gives a world: the match's entity, and each player's entity and
/// resources.
pub trait PlayersWorld {
    /// The entity of player `slot`, once it spawned.
    fn player_entity(&self, slot: PlayerSlot) -> Option<Entity>;

    /// The resources of player `slot`: none for a player the session lacks, or in a match that
    /// keeps no players' resources.
    fn player_resources(&self, slot: PlayerSlot) -> Option<&PlayerResources>;
}

impl PlayersWorld for World {
    fn player_entity(&self, slot: PlayerSlot) -> Option<Entity> {
        self.entity_of(StableId::of_player(slot))
    }

    fn player_resources(&self, slot: PlayerSlot) -> Option<&PlayerResources> {
        self.get(self.player_entity(slot)?)
    }
}

/// What the players module gives a world, for its own crate.
pub trait PlayersWorldCrate {
    /// The resources of the player that owns a unit of `owner`: none for a unit no
    /// player owns, or in a match that keeps no players' resources.
    fn owner_resources<'w>(&'w self, owner: Option<&Owner>) -> Option<&'w PlayerResources>;

    /// The resources of player `slot`, to change; none in a match that keeps no
    /// players' resources.
    fn player_resources_mut(&mut self, slot: PlayerSlot) -> Option<Mut<'_, PlayerResources>>;

    /// How many resources each player of the match holds an amount of: those its
    /// mode declares, none before a mode.
    fn declared_resources(&self) -> usize;
}

impl PlayersWorldCrate for World {
    fn owner_resources<'w>(&'w self, owner: Option<&Owner>) -> Option<&'w PlayerResources> {
        self.player_resources(owner?.slot())
    }

    fn player_resources_mut(&mut self, slot: PlayerSlot) -> Option<Mut<'_, PlayerResources>> {
        let entity = self.player_entity(slot)?;
        self.get_mut(entity)
    }

    fn declared_resources(&self) -> usize {
        self.non_send::<View>().names().resources.len()
    }
}
