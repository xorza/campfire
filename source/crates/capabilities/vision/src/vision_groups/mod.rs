use campfire_values::{NameId, Relations, Team, TeamSet};

/// The match's vision groups: the teams that share vision, each group seeing as one. Derived
/// from the relations, never state: built again whenever they change.
#[derive(Debug, Default)]
pub(crate) struct VisionGroups {
    /// Each team's group, by team index.
    group_of: Vec<u8>,
    /// Each group's teams, groups numbered in the order of their lowest team.
    members: Vec<TeamSet>,
}

impl VisionGroups {
    /// Builds the groups of `teams` teams again from `relations`: two teams that share vision are
    /// of one group, and so are the teams either shares it with.
    pub(crate) fn rebuild(&mut self, teams: usize, relations: &Relations) {
        self.group_of.clear();
        self.members.clear();
        debug_assert!(teams <= Team::LIMIT, "teams fit their ids");
        let lowest = relations.vision_lowest();
        for (index, low) in (0..=u8::MAX).zip(lowest).take(teams) {
            let group = if low == index {
                self.members.push(TeamSet::NONE);
                u8::try_from(self.members.len() - 1).expect("groups fit u8")
            } else {
                self.group_of[usize::from(low)]
            };
            let members = &mut self.members[usize::from(group)];
            *members = members.with(Team::new(index));
            self.group_of.push(group);
        }
    }

    /// How many groups there are.
    pub(crate) const fn count(&self) -> usize {
        self.members.len()
    }

    /// The group of `team`.
    pub(crate) fn of(&self, team: Team) -> usize {
        usize::from(self.group_of[team.index()])
    }

    /// The teams of group `group`.
    pub(crate) fn members(&self, group: usize) -> TeamSet {
        self.members[group]
    }
}

#[cfg(test)]
mod tests;
