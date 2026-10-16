use std::sync::Arc;

use campfire_common::PlayerSlot;
use campfire_values::{Names, Team};

use crate::scripts::error::{ApiError, Checked};

/// The match's teams, in the manifest's order, the playing teams among them, those with slots,
/// and each player's team by slot. A team with no slots, such as a MOBA's camps, has only units.
/// Package data, not state: the mode builds it, and the mode and the script view share it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Teams {
    /// Every team's name, by team; shared with the script view.
    names: Arc<Names<Team>>,
    /// The teams with slots, in order.
    playing: Vec<Team>,
    /// Each player's team, by slot.
    slots: Vec<Team>,
}

impl Teams {
    /// The teams named `names`, each with its slot count of `slots`, for `players` players,
    /// whose slots fill the teams in order; `None` when the teams have fewer slots than players.
    pub fn new(
        names: Names<Team>,
        slots: impl IntoIterator<Item = u32>,
        players: u32,
    ) -> Option<Self> {
        let mut built = Self::default();
        let mut unseated = players;
        let teams = names.iter().map(|(team, _)| team).zip(slots);
        for (team, slots) in teams {
            if slots > 0 {
                built.playing.push(team);
            }
            // Only the players' slots are kept, so a team of many slots costs no more.
            let seated = slots.min(unseated);
            built.slots.extend((0..seated).map(|_| team));
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

    /// The other playing team of `team`, in a match of two playing teams.
    pub fn sole_enemy(&self, team: Team) -> Option<Team> {
        match self.playing[..] {
            [a, b] if team == a => Some(b),
            [a, b] if team == b => Some(a),
            _ => None,
        }
    }

    /// The team of player `slot`.
    pub fn of(&self, slot: PlayerSlot) -> Option<Team> {
        self.slots.get(slot.index()).copied()
    }

    pub fn players(&self) -> u32 {
        u32::try_from(self.slots.len()).expect("players fit u32")
    }

    /// Player `player`'s slot, as a script names it, when the session has it.
    pub(crate) fn player(&self, player: i64) -> Checked<PlayerSlot> {
        u32::try_from(player)
            .ok()
            .filter(|&slot| slot < self.players())
            .map(PlayerSlot::new)
            .ok_or_else(|| ApiError::UnknownPlayer.fail().into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_fill_the_teams_in_order_and_a_team_with_none_does_not_play() {
        let build = |names: &[&str], slots: &[u32], players| {
            let names = names.iter().copied().collect();
            Teams::new(names, slots.iter().copied(), players)
        };
        let teams = build(&["a", "camps", "b"], &[2, 0, 1], 3).unwrap();
        let team = Team::new;
        let slots = [0, 1, 2].map(|slot| teams.of(PlayerSlot::new(slot)));
        assert_eq!(slots, [Some(team(0)), Some(team(0)), Some(team(2))]);
        assert_eq!(teams.of(PlayerSlot::new(3)), None);
        assert_eq!(teams.players(), 3);
        assert_eq!(teams.playing(), [team(0), team(2)]);
        assert_eq!(teams.count(), 3);
        let named = ["a", "b", "camps", "c"].map(|name| teams.names().named(name));
        assert_eq!(named, [Some(team(0)), Some(team(2)), Some(team(1)), None]);
        assert_eq!(teams.names().name(team(1)), Some("camps"));
        assert_eq!(teams.names().name(team(3)), None);
        // In a match of two playing teams, each has the other; a team that does not play has
        // none.
        let enemies = [0, 1, 2].map(|at| teams.sole_enemy(team(at)));
        assert_eq!(enemies, [Some(team(2)), None, Some(team(0))]);
        // Fewer players than slots leave the last slots empty; more players than slots fail.
        assert_eq!(build(&["a", "b"], &[2, 1], 1).unwrap().players(), 1);
        assert_eq!(build(&["a", "b"], &[2, 1], 4), None);
        // A team of 2³² − 1 slots seats the 2 players and keeps 2 slots, both its own.
        let wide = build(&["a", "b"], &[u32::MAX, 1], 2).unwrap();
        assert_eq!(wide.slots, [team(0), team(0)]);
        assert_eq!(wide.playing(), [team(0), team(1)]);
        assert!(Teams::default().playing().is_empty());
        // A battle royale's 100 one-player teams: player 99 on team 99, every team playing.
        let solo_names: Vec<String> = (0..100).map(|at| format!("solo{at}")).collect();
        let solo_table = solo_names.iter().map(String::as_str).collect();
        let solos = Teams::new(solo_table, [1; 100], 100).unwrap();
        assert_eq!(solos.of(PlayerSlot::new(99)), Some(team(99)));
        assert_eq!((solos.count(), solos.playing().len()), (100, 100));
        assert_eq!(solos.sole_enemy(team(0)), None);
    }
}
