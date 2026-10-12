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
    /// The match's entity, once it spawned.
    fn match_entity(&self) -> Option<Entity>;

    /// The entity of player `slot`, once it spawned.
    fn player_entity(&self, slot: PlayerSlot) -> Option<Entity>;

    /// The resources of player `slot`: none for a player the session lacks, or in a match that
    /// keeps no players' resources.
    fn player_resources(&self, slot: PlayerSlot) -> Option<&PlayerResources>;
}

impl PlayersWorld for World {
    fn match_entity(&self) -> Option<Entity> {
        self.entity_of(StableId::MATCH)
    }

    fn player_entity(&self, slot: PlayerSlot) -> Option<Entity> {
        self.entity_of(StableId::of_player(slot))
    }

    fn player_resources(&self, slot: PlayerSlot) -> Option<&PlayerResources> {
        self.get(self.player_entity(slot)?)
    }
}

/// What the players module gives a world, for its own crate.
pub(crate) trait PlayersWorldCrate {
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
        self.non_send::<View>().resource_count()
    }
}

#[cfg(test)]
mod tests {
    use std::panic;

    use campfire_sim::Capability;

    use super::*;
    use crate::players::Players;
    use crate::players::player::Player;
    use crate::release::r#match::Match;
    use crate::stats::player_modifiers::PlayerModifiers;
    use crate::units::relations::Relations;
    use crate::units::team::Team;

    #[test]
    fn a_match_with_no_mode_starts_with_its_entity_then_its_players_and_none_joins_one_unstarted() {
        // Two players, of teams 1 then 0, with two resources each: the match's entity holds
        // relations of hostile teams, and each player's its slot, its team, the stats' part and
        // two resources at 0.
        let mut game = Match::test_unstarted(&[Capability::Stats], Match::TEST_RATE, None);
        Players::start_without_mode(&mut game, [Team::new(1), Team::new(0)], 2);
        let entity = game.match_entity().unwrap();
        assert_eq!(game.get::<Relations>(entity), Some(&Relations::default()));
        for (slot, team) in [(0, 1), (1, 0)] {
            let slot = PlayerSlot::new(slot);
            let player = game.entity(game.player_entity(slot).unwrap());
            assert_eq!(player.get::<Player>(), Some(&Player::new(slot)));
            assert_eq!(player.get::<Team>(), Some(&Team::new(team)));
            assert_eq!(
                player.get::<PlayerModifiers>(),
                Some(&PlayerModifiers::default())
            );
            assert_eq!(game.player_resources(slot), Some(&PlayerResources::new(2)));
        }
        assert_eq!(game.player_entity(PlayerSlot::new(2)), None);

        // A player joins only a match that started: one of no match entity refuses it.
        let mut unstarted = Match::test_unstarted(&[Capability::Stats], Match::TEST_RATE, None);
        let joined = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            Players::spawn(&mut unstarted, 1, 0);
        }));
        let refusal = joined.unwrap_err();
        assert_eq!(
            refusal.downcast_ref::<&str>(),
            Some(&"players join a match that started, which holds its entity")
        );
    }
}
