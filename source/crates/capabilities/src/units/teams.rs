use campfire_math::PlayerSlot;
use campfire_script::rhai::INT;

use crate::scripts::error::{ApiError, Checked};
use crate::units::team::Team;
use crate::values::name_list::NameList;

/// The match's teams, in the manifest's order, the playing teams among them, those with slots,
/// and each player's team by slot. A team with no slots, such as a MOBA's camps, has only units.
/// Package data, not state: the mode builds it, and the mode and the script view share it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Teams {
    /// Every team's name, by index.
    names: NameList,
    /// The teams with slots, in order.
    playing: Vec<Team>,
    /// Each player's team, by slot.
    slots: Vec<Team>,
}

impl Teams {
    /// The teams of `teams`, each a name and its slot count, for `players` players, whose slots
    /// fill the teams in order; `None` when the teams have fewer slots than players.
    pub(crate) fn new<'a>(
        teams: impl IntoIterator<Item = (&'a str, u32)>,
        players: u32,
    ) -> Option<Teams> {
        let mut built = Teams::default();
        let mut unseated = players;
        for (index, (name, slots)) in teams.into_iter().enumerate() {
            built.names.push(name);
            let team = Team::new(u8::try_from(index).expect("teams fit u8"));
            if slots > 0 {
                built.playing.push(team);
            }
            // Only the players' slots are kept, so a team of many slots costs no more.
            let seated = slots.min(unseated);
            built.slots.extend((0..seated).map(|_| team));
            unseated -= seated;
        }
        (unseated == 0).then_some(built)
    }

    /// The team named `name`.
    pub(crate) fn named(&self, name: &str) -> Option<Team> {
        let index = self.names.named(name)?;
        Some(Team::new(u8::try_from(index).expect("teams fit u8")))
    }

    pub(crate) fn name(&self, team: Team) -> Option<&str> {
        self.names.get(usize::from(team.index()))
    }

    /// How many teams the match holds.
    pub(crate) const fn count(&self) -> usize {
        self.names.len()
    }

    /// The playing teams, in order.
    pub(crate) fn playing(&self) -> &[Team] {
        &self.playing
    }

    /// The other playing team of `team`, in a match of two playing teams.
    pub(crate) fn sole_enemy(&self, team: Team) -> Option<Team> {
        match self.playing[..] {
            [a, b] if team == a => Some(b),
            [a, b] if team == b => Some(a),
            _ => None,
        }
    }

    /// The team of player `slot`.
    pub(crate) fn of(&self, slot: PlayerSlot) -> Option<Team> {
        self.slots.get(slot.index()).copied()
    }

    pub(crate) fn players(&self) -> u32 {
        u32::try_from(self.slots.len()).expect("players fit u32")
    }

    /// Player `player`'s slot, as a script names it, when the session has it.
    pub(crate) fn player(&self, player: INT) -> Checked<PlayerSlot> {
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
        let teams = Teams::new([("a", 2), ("camps", 0), ("b", 1)], 3).unwrap();
        let team = Team::new;
        let slots = [0, 1, 2].map(|slot| teams.of(PlayerSlot::new(slot)));
        assert_eq!(slots, [Some(team(0)), Some(team(0)), Some(team(2))]);
        assert_eq!(teams.of(PlayerSlot::new(3)), None);
        assert_eq!(teams.players(), 3);
        assert_eq!(teams.playing(), [team(0), team(2)]);
        assert_eq!(teams.count(), 3);
        let named = ["a", "b", "camps", "c"].map(|name| teams.named(name));
        assert_eq!(named, [Some(team(0)), Some(team(2)), Some(team(1)), None]);
        assert_eq!(teams.name(team(1)), Some("camps"));
        assert_eq!(teams.name(team(3)), None);
        // In a match of two playing teams, each has the other; a team that does not play has
        // none.
        let enemies = [0, 1, 2].map(|at| teams.sole_enemy(team(at)));
        assert_eq!(enemies, [Some(team(2)), None, Some(team(0))]);
        // Fewer players than slots leave the last slots empty; more players than slots fail.
        assert_eq!(Teams::new([("a", 2), ("b", 1)], 1).unwrap().players(), 1);
        assert_eq!(Teams::new([("a", 2), ("b", 1)], 4), None);
        // A team of 2³² − 1 slots seats the 2 players and keeps 2 slots, both its own.
        let wide = Teams::new([("a", u32::MAX), ("b", 1)], 2).unwrap();
        assert_eq!(wide.slots, [team(0), team(0)]);
        assert_eq!(wide.playing(), [team(0), team(1)]);
        assert!(Teams::default().playing().is_empty());
        // A battle royale's 100 one-player teams: player 99 on team 99, every team playing.
        let solo_names: Vec<String> = (0..100).map(|at| format!("solo{at}")).collect();
        let solos = Teams::new(solo_names.iter().map(|name| (name.as_str(), 1)), 100).unwrap();
        assert_eq!(solos.of(PlayerSlot::new(99)), Some(team(99)));
        assert_eq!((solos.count(), solos.playing().len()), (100, 100));
        assert_eq!(solos.sole_enemy(team(0)), None);
    }
}
