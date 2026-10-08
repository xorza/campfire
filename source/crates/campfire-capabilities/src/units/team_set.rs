use serde::{Deserialize, Serialize};

use crate::units::bits256::Bits256;
use crate::units::team::Team;

/// A set of teams, one bit each: a match holds at most `Team::LIMIT` teams.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TeamSet(Bits256);

impl TeamSet {
    pub(crate) const NONE: TeamSet = TeamSet(Bits256::NONE);
    pub const ALL: TeamSet = TeamSet(Bits256::ALL);

    #[must_use]
    pub(crate) const fn with(self, team: Team) -> TeamSet {
        TeamSet(self.0.with(team.index() as usize))
    }

    /// The teams in either set.
    #[must_use]
    pub(crate) const fn union(self, other: TeamSet) -> TeamSet {
        TeamSet(self.0.union(other.0))
    }

    pub const fn contains(self, team: Team) -> bool {
        self.0.contains(team.index() as usize)
    }

    /// Its teams, in order.
    pub fn teams(self) -> impl Iterator<Item = Team> {
        self.0
            .iter()
            .map(|index| Team::new(u8::try_from(index).expect("a team's bit fits its index")))
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use crate::units::team::Team;
    use crate::units::team_set::TeamSet;

    impl TeamSet {
        /// The set of `team` alone.
        pub(crate) const fn of(team: Team) -> TeamSet {
            TeamSet::NONE.with(team)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(union.teams().collect::<Vec<_>>(), [a, c, d]);
        assert_eq!(TeamSet::NONE.teams().count(), 0);
        assert_eq!(TeamSet::ALL.teams().count(), Team::LIMIT);
    }
}
