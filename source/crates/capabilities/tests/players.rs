use std::panic;

use campfire_capabilities::Match;
use campfire_common::PlayerSlot;
use campfire_players::*;
use campfire_sim::Capability;
use campfire_stats::PlayerModifiers;
use campfire_units::{Player, UnitsWorld};
use campfire_values::{Relations, Team};

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
