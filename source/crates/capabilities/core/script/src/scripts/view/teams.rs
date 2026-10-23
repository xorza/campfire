use std::sync::Arc;

use campfire_common::PlayerSlot;
use campfire_values::{Names, Team};

/// The match's teams, in the manifest's order, the playing teams among them, those with player
/// slots, and each player's team by player slot. A team with no player slots, such as a MOBA's
/// camps, has only units. Package data, not state: the mode builds it, and the mode and the script
/// view share it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Teams {
    /// Every team's name, by team; shared with the script view.
    names: Arc<Names<Team>>,
    /// The teams with player slots, in order.
    playing: Vec<Team>,
    /// Each player's team, by player slot.
    player_slots: Vec<Team>,
}

impl Teams {
    /// The teams named `names`, each with its player slot count of `player_slots`, for `players`
    /// players, whose player slots fill the teams in order; `None` when the teams have fewer player
    /// slots than players.
    pub fn new(
        names: Names<Team>,
        player_slots: impl IntoIterator<Item = u32>,
        players: u32,
    ) -> Option<Self> {
        let mut built = Self::default();
        let mut unheld = players;
        let teams = names.iter().map(|(team, _)| team).zip(player_slots);
        for (team, player_slots) in teams {
            if player_slots > 0 {
                built.playing.push(team);
            }
            // Only the players' player slots are kept, so a team of many player slots costs no
            // more.
            let held = player_slots.min(unheld);
            built.player_slots.extend((0..held).map(|_| team));
            unheld -= held;
        }
        built.names = Arc::new(names);
        (unheld == 0).then_some(built)
    }

    /// The teams' names.
    pub const fn names(&self) -> &Arc<Names<Team>> {
        &self.names
    }

    /// How many teams the match holds.
    pub(crate) fn count(&self) -> usize {
        self.names.len()
    }

    /// The playing teams, in order.
    pub fn playing(&self) -> &[Team] {
        &self.playing
    }

    /// The team of player `player_slot`.
    pub fn of(&self, player_slot: PlayerSlot) -> Option<Team> {
        self.player_slots.get(player_slot.index()).copied()
    }

    pub fn players(&self) -> u32 {
        u32::try_from(self.player_slots.len()).expect("players fit u32")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_slots_fill_the_teams_in_order_and_a_team_with_none_does_not_play() {
        let build = |names: &[&str], player_slots: &[u32], players| {
            let names = names.iter().copied().collect();
            Teams::new(names, player_slots.iter().copied(), players)
        };
        let teams = build(&["a", "camps", "b"], &[2, 0, 1], 3).unwrap();
        let team = Team::new;
        let player_slots = [0, 1, 2].map(|player_slot| teams.of(PlayerSlot::new(player_slot)));
        assert_eq!(player_slots, [Some(team(0)), Some(team(0)), Some(team(2))]);
        assert_eq!(teams.of(PlayerSlot::new(3)), None);
        assert_eq!(teams.players(), 3);
        assert_eq!(teams.playing(), [team(0), team(2)]);
        assert_eq!(teams.count(), 3);
        let named = ["a", "b", "camps", "c"].map(|name| teams.names().named(name));
        assert_eq!(named, [Some(team(0)), Some(team(2)), Some(team(1)), None]);
        assert_eq!(teams.names().name(team(1)), Some("camps"));
        assert_eq!(teams.names().name(team(3)), None);
        // Fewer players than player slots leave the last player slots empty; more players than
        // player slots fail.
        assert_eq!(build(&["a", "b"], &[2, 1], 1).unwrap().players(), 1);
        assert_eq!(build(&["a", "b"], &[2, 1], 4), None);
        // A team of 2³² − 1 player slots places the 2 players and keeps 2 player slots, both its
        // own.
        let wide = build(&["a", "b"], &[u32::MAX, 1], 2).unwrap();
        assert_eq!(wide.player_slots, [team(0), team(0)]);
        assert_eq!(wide.playing(), [team(0), team(1)]);
        assert!(Teams::default().playing().is_empty());
        // A battle royale's 100 one-player teams: player 99 on team 99, every team playing.
        let solo_names: Vec<String> = (0..100).map(|at| format!("solo{at}")).collect();
        let solo_table = solo_names.iter().map(String::as_str).collect();
        let solos = Teams::new(solo_table, [1; 100], 100).unwrap();
        assert_eq!(solos.of(PlayerSlot::new(99)), Some(team(99)));
        assert_eq!((solos.count(), solos.playing().len()), (100, 100));
    }
}
