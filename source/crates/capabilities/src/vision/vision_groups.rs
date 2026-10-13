use crate::units::relations::Relations;
use crate::units::team::Team;
use crate::units::team_set::TeamSet;

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
mod tests {
    use super::*;
    use crate::values::relation::Relation;

    #[test]
    fn friends_that_share_vision_form_one_group_and_vision_off_splits_them() {
        // Five teams: 0 and 3 friends, 3 and 4 friends, so 0, 3 and 4 see as one; 1 and 2
        // friends with vision off see apart.
        let team = Team::new;
        let mut relations = Relations::default();
        relations.set(team(0), team(3), Relation::Friendly, true);
        relations.set(team(4), team(3), Relation::Friendly, true);
        relations.set(team(1), team(2), Relation::Friendly, false);
        relations.set(team(0), team(1), Relation::Neutral, true);
        let mut groups = VisionGroups::default();
        groups.rebuild(5, &relations);
        assert_eq!(groups.count(), 3);
        assert_eq!(
            [0, 1, 2, 3, 4].map(|at| groups.of(team(at))),
            [0, 1, 2, 0, 0]
        );
        let first = TeamSet::of(team(0)).with(team(3)).with(team(4));
        assert_eq!(groups.members(0), first);
        assert_eq!(groups.members(2), TeamSet::of(team(2)));
        // A chain whose later link reaches a lower team: 2 with 5, then 1 with 5, makes 1, 2
        // and 5 one group, numbered after 0's, as each team's own group says too.
        let mut relations = Relations::default();
        relations.set(team(2), team(5), Relation::Friendly, true);
        relations.set(team(1), team(5), Relation::Friendly, true);
        groups.rebuild(6, &relations);
        assert_eq!(
            [0, 1, 2, 3, 4, 5].map(|at| groups.of(team(at))),
            [0, 1, 1, 2, 3, 1]
        );
        assert_eq!(
            groups.members(1),
            TeamSet::of(team(1)).with(team(2)).with(team(5))
        );
        for at in 0..6 {
            let own = groups.members(groups.of(team(at)));
            assert_eq!(relations.vision_group(team(at)), own, "{at}");
        }
    }
}
