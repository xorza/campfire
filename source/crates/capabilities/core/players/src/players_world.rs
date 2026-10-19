use bevy_ecs::entity::Entity;
use bevy_ecs::world::{Mut, World};
use campfire_common::Seat;
use campfire_script::MatchNames;
use campfire_sim::{SimWorld, StableId};
use campfire_units::Owner;
use campfire_values::ResourceId;

use crate::player_resources::PlayerResources;

/// What the players module gives a world, for every crate: each player's entity and resources.
pub trait PlayersWorld {
    /// The entity of player `seat`, once it spawned.
    fn player_entity(&self, seat: Seat) -> Option<Entity>;

    /// The resources of player `seat`, on its entity: none for a player the session lacks, or in
    /// a match that keeps no players' resources.
    fn player_resources(&self, seat: Seat) -> Option<&PlayerResources>;

    /// The resources of the player that owns a unit of `owner`: none for a unit no
    /// player owns, or in a match that keeps no players' resources.
    fn owner_resources<'w>(&'w self, owner: Option<&Owner>) -> Option<&'w PlayerResources>;

    /// The resources of player `seat`, to change; none in a match that keeps no
    /// players' resources.
    fn player_resources_mut(&mut self, seat: Seat) -> Option<Mut<'_, PlayerResources>>;

    /// How many resources each player of the match holds an amount of: those its
    /// mode declares, none before a mode.
    fn declared_resources(&self) -> usize;
}

impl PlayersWorld for World {
    fn player_entity(&self, seat: Seat) -> Option<Entity> {
        self.entity_of(StableId::of_player(seat))
    }

    fn player_resources(&self, seat: Seat) -> Option<&PlayerResources> {
        self.get(self.player_entity(seat)?)
    }

    fn owner_resources<'w>(&'w self, owner: Option<&Owner>) -> Option<&'w PlayerResources> {
        self.player_resources(owner?.seat())
    }

    fn player_resources_mut(&mut self, seat: Seat) -> Option<Mut<'_, PlayerResources>> {
        let entity = self.player_entity(seat)?;
        self.get_mut(entity)
    }

    fn declared_resources(&self) -> usize {
        self.resource::<MatchNames>().count::<ResourceId>()
    }
}
