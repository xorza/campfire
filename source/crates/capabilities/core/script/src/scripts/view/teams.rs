use std::sync::Arc;

use campfire_common::Seat;
use campfire_values::{Names, Team};

/// The match's teams, in the manifest's order, the playing teams among them, those with seats,
/// and each player's team by seat. A team with no seats, such as a MOBA's camps, has only units.
/// Package data, not state: the mode builds it, and the mode and the script view share it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Teams {
    /// Every team's name, by team; shared with the script view.
    names: Arc<Names<Team>>,
    /// The teams with seats, in order.
    playing: Vec<Team>,
    /// Each player's team, by seat.
    seats: Vec<Team>,
}

impl Teams {
    /// The teams named `names`, each with its seat count of `seats`, for `players` players,
    /// whose seats fill the teams in order; `None` when the teams have fewer seats than players.
    pub fn new(
        names: Names<Team>,
        seats: impl IntoIterator<Item = u32>,
        players: u32,
    ) -> Option<Self> {
        let mut built = Self::default();
        let mut unseated = players;
        let teams = names.iter().map(|(team, _)| team).zip(seats);
        for (team, seats) in teams {
            if seats > 0 {
                built.playing.push(team);
            }
            // Only the players' seats are kept, so a team of many seats costs no more.
            let seated = seats.min(unseated);
            built.seats.extend((0..seated).map(|_| team));
            unseated -= seated;
        }
        built.names = Arc::new(names);
        (unseated == 0).then_some(built)
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

    /// The team of player `seat`.
    pub fn of(&self, seat: Seat) -> Option<Team> {
        self.seats.get(seat.index()).copied()
    }

    pub fn players(&self) -> u32 {
        u32::try_from(self.seats.len()).expect("players fit u32")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seats_fill_the_teams_in_order_and_a_team_with_none_does_not_play() {
        let build = |names: &[&str], seats: &[u32], players| {
            let names = names.iter().copied().collect();
            Teams::new(names, seats.iter().copied(), players)
        };
        let teams = build(&["a", "camps", "b"], &[2, 0, 1], 3).unwrap();
        let team = Team::new;
        let seats = [0, 1, 2].map(|seat| teams.of(Seat::new(seat)));
        assert_eq!(seats, [Some(team(0)), Some(team(0)), Some(team(2))]);
        assert_eq!(teams.of(Seat::new(3)), None);
        assert_eq!(teams.players(), 3);
        assert_eq!(teams.playing(), [team(0), team(2)]);
        assert_eq!(teams.count(), 3);
        let named = ["a", "b", "camps", "c"].map(|name| teams.names().named(name));
        assert_eq!(named, [Some(team(0)), Some(team(2)), Some(team(1)), None]);
        assert_eq!(teams.names().name(team(1)), Some("camps"));
        assert_eq!(teams.names().name(team(3)), None);
        // Fewer players than seats leave the last seats empty; more players than seats fail.
        assert_eq!(build(&["a", "b"], &[2, 1], 1).unwrap().players(), 1);
        assert_eq!(build(&["a", "b"], &[2, 1], 4), None);
        // A team of 2³² − 1 seats seats the 2 players and keeps 2 seats, both its own.
        let wide = build(&["a", "b"], &[u32::MAX, 1], 2).unwrap();
        assert_eq!(wide.seats, [team(0), team(0)]);
        assert_eq!(wide.playing(), [team(0), team(1)]);
        assert!(Teams::default().playing().is_empty());
        // A battle royale's 100 one-player teams: player 99 on team 99, every team playing.
        let solo_names: Vec<String> = (0..100).map(|at| format!("solo{at}")).collect();
        let solo_table = solo_names.iter().map(String::as_str).collect();
        let solos = Teams::new(solo_table, [1; 100], 100).unwrap();
        assert_eq!(solos.of(Seat::new(99)), Some(team(99)));
        assert_eq!((solos.count(), solos.playing().len()), (100, 100));
    }
}
