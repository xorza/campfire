use campfire_math::PlayerSlot;

use crate::units::team::Team;

/// The match's teams: the playing teams in the manifest's order, then the neutral one, and each
/// player's team by slot. Package data, not state: the mode builds it, and the mode and the
/// script view share it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Teams {
    /// Every team's name, by index; the neutral team's is last.
    names: Vec<Box<str>>,
    /// Each player's team, by slot.
    slots: Vec<Team>,
}

impl Teams {
    /// The name of the team neutral units spawn on: one more team, an enemy of every other.
    pub(crate) const NEUTRAL: &str = "neutral";

    /// The playing teams of `playing`, each a name and its slot count, for `players` players,
    /// whose slots fill the teams in order; `None` when the teams have fewer slots than players.
    pub(crate) fn new<'a>(
        playing: impl IntoIterator<Item = (&'a str, u32)>,
        players: u32,
    ) -> Option<Teams> {
        let mut teams = Teams::default();
        for (index, (name, slots)) in playing.into_iter().enumerate() {
            teams.names.push(name.into());
            let team = Team::new(u8::try_from(index).expect("teams fit u8"));
            teams.slots.extend((0..slots).map(|_| team));
        }
        teams.names.push(Teams::NEUTRAL.into());
        let players = usize::try_from(players).expect("players fit usize");
        if teams.slots.len() < players {
            return None;
        }
        teams.slots.truncate(players);
        Some(teams)
    }

    /// The team named `name`: a playing team, or the neutral one.
    pub(crate) fn named(&self, name: &str) -> Option<Team> {
        let index = self.names.iter().position(|held| **held == *name)?;
        Some(Team::new(u8::try_from(index).expect("teams fit u8")))
    }

    pub(crate) fn name(&self, team: Team) -> Option<&str> {
        self.names
            .get(usize::from(team.index()))
            .map(|name| &**name)
    }

    /// How many teams the match holds, the neutral one included.
    pub(crate) fn count(&self) -> usize {
        self.names.len()
    }

    /// The names of the playing teams, in order.
    pub(crate) fn playing(&self) -> impl ExactSizeIterator<Item = &str> {
        self.names[..self.names.len().saturating_sub(1)]
            .iter()
            .map(|name| &**name)
    }

    /// The one enemy team of `team`, in a match of two playing teams.
    pub(crate) fn sole_enemy(&self, team: Team) -> Option<Team> {
        match (team.index(), self.playing().len()) {
            (0, 2) => Some(Team::new(1)),
            (1, 2) => Some(Team::new(0)),
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_fill_the_teams_in_order_and_the_neutral_team_comes_last() {
        let teams = Teams::new([("a", 2), ("b", 1)], 3).unwrap();
        let team = Team::new;
        let slots = [0, 1, 2].map(|slot| teams.of(PlayerSlot::new(slot)));
        assert_eq!(slots, [Some(team(0)), Some(team(0)), Some(team(1))]);
        assert_eq!(teams.of(PlayerSlot::new(3)), None);
        assert_eq!(teams.players(), 3);
        assert_eq!(teams.playing().collect::<Vec<_>>(), ["a", "b"]);
        let named = ["a", "b", "neutral", "c"].map(|name| teams.named(name));
        assert_eq!(named, [Some(team(0)), Some(team(1)), Some(team(2)), None]);
        assert_eq!(teams.name(team(2)), Some("neutral"));
        assert_eq!(teams.name(team(3)), None);
        // Fewer players than slots leave the last slots empty; more players than slots fail.
        assert_eq!(Teams::new([("a", 2), ("b", 1)], 1).unwrap().players(), 1);
        assert_eq!(Teams::new([("a", 2), ("b", 1)], 4), None);
        assert_eq!(Teams::default().playing().count(), 0);
    }
}
