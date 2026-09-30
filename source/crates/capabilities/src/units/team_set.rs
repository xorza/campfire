use serde::{Deserialize, Serialize};

use crate::units::team::Team;

/// A set of teams, one bit each: a match holds at most `Team::LIMIT` teams.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TeamSet(u64);

impl TeamSet {
    pub(crate) const NONE: TeamSet = TeamSet(0);
    pub const ALL: TeamSet = TeamSet(u64::MAX);

    pub const fn of(team: Team) -> TeamSet {
        TeamSet::NONE.with(team)
    }

    #[must_use]
    pub(crate) const fn with(self, team: Team) -> TeamSet {
        TeamSet(self.0 | 1 << team.index())
    }

    /// The teams in either set.
    #[must_use]
    pub const fn union(self, other: TeamSet) -> TeamSet {
        TeamSet(self.0 | other.0)
    }

    pub const fn contains(self, team: Team) -> bool {
        self.0 & 1 << team.index() != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_union_holds_the_teams_of_either_set() {
        let [a, b, c] = [0, 1, 63].map(Team::new);
        let union = TeamSet::of(a).union(TeamSet::of(c));
        assert_eq!(
            [a, b, c].map(|team| union.contains(team)),
            [true, false, true]
        );
        assert_eq!(TeamSet::NONE.union(TeamSet::NONE), TeamSet::NONE);
        assert_eq!(union.union(TeamSet::ALL), TeamSet::ALL);
    }
}
