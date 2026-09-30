use serde::{Deserialize, Serialize};

use crate::units::team::Team;

/// A set of teams, one bit each: a match holds at most `Team::LIMIT` teams.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TeamSet(u64);

impl TeamSet {
    pub(crate) const NONE: TeamSet = TeamSet(0);
    pub(crate) const ALL: TeamSet = TeamSet(u64::MAX);

    pub(crate) const fn of(team: Team) -> TeamSet {
        TeamSet::NONE.with(team)
    }

    #[must_use]
    pub(crate) const fn with(self, team: Team) -> TeamSet {
        TeamSet(self.0 | 1 << team.index())
    }

    pub const fn contains(self, team: Team) -> bool {
        self.0 & 1 << team.index() != 0
    }
}
