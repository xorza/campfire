use serde::{Deserialize, Serialize};

use crate::bits256::Bits256;
use crate::team::Team;

/// A set of teams, one bit each: a match holds at most `Team::LIMIT` teams.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TeamSet(Bits256);

impl TeamSet {
    pub const NONE: TeamSet = TeamSet(Bits256::NONE);
    pub const ALL: TeamSet = TeamSet(Bits256::ALL);

    #[must_use]
    pub const fn with(self, team: Team) -> TeamSet {
        TeamSet(self.0.with(team.get() as usize))
    }

    /// The teams in either set.
    #[must_use]
    pub const fn union(self, other: TeamSet) -> TeamSet {
        TeamSet(self.0.union(other.0))
    }

    pub const fn contains(self, team: Team) -> bool {
        self.0.contains(team.get() as usize)
    }

    /// Its teams, in order.
    pub fn iter(self) -> impl Iterator<Item = Team> {
        self.0
            .iter()
            .map(|index| Team::new(u8::try_from(index).expect("a team's bit fits its index")))
    }
}

impl FromIterator<Team> for TeamSet {
    fn from_iter<I: IntoIterator<Item = Team>>(teams: I) -> TeamSet {
        teams.into_iter().fold(TeamSet::NONE, TeamSet::with)
    }
}

impl<const N: usize> From<[Team; N]> for TeamSet {
    fn from(teams: [Team; N]) -> TeamSet {
        teams.into_iter().collect()
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::team::Team;
    use crate::team_set::TeamSet;

    impl TeamSet {
        /// The set of `team` alone.
        pub const fn of(team: Team) -> TeamSet {
            TeamSet::NONE.with(team)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::names::NameId;

    #[test]
    fn a_union_holds_the_teams_of_either_set() {
        let [a, b, c, d] = [0, 1, 63, 255].map(Team::new);
        let union = TeamSet::of(a).union(TeamSet::of(c)).union(TeamSet::of(d));
        assert_eq!(
            [a, b, c, d].map(|team| union.contains(team)),
            [true, false, true, true]
        );
        assert!(!TeamSet::of(Team::new(64)).contains(Team::new(0)));
        assert_eq!(TeamSet::NONE.union(TeamSet::NONE), TeamSet::NONE);
        assert_eq!(union.union(TeamSet::ALL), TeamSet::ALL);
        assert_eq!(union.iter().collect::<Vec<_>>(), [a, c, d]);
        assert_eq!(TeamSet::NONE.iter().count(), 0);
        assert_eq!(TeamSet::ALL.iter().count(), Team::LIMIT);
    }
}
