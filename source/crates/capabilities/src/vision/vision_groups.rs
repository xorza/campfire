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
        let mut root: Vec<usize> = (0..teams).collect();
        let find = |root: &mut Vec<usize>, mut at: usize| {
            while root[at] != at {
                root[at] = root[root[at]];
                at = root[at];
            }
            at
        };
        for (a, b) in relations.vision_pairs() {
            let (a, b) = (usize::from(a.index()), usize::from(b.index()));
            if a < teams && b < teams {
                let (a, b) = (find(&mut root, a), find(&mut root, b));
                root[a.max(b)] = a.min(b);
            }
        }
        self.group_of.clear();
        self.members.clear();
        for at in 0..teams {
            let lowest = find(&mut root, at);
            let team = Team::new(u8::try_from(at).expect("teams fit u8"));
            let group = if lowest == at {
                self.members.push(TeamSet::of(team));
                self.members.len() - 1
            } else {
                usize::from(self.group_of[lowest])
            };
            self.group_of
                .push(u8::try_from(group).expect("groups fit u8"));
            self.members[group] = self.members[group].with(team);
        }
    }

    /// How many groups there are.
    pub(crate) fn count(&self) -> usize {
        self.members.len()
    }

    /// The group of `team`.
    pub(crate) fn of(&self, team: Team) -> usize {
        usize::from(self.group_of[usize::from(team.index())])
    }

    /// The teams of group `group`.
    pub(crate) fn members(&self, group: usize) -> TeamSet {
        self.members[group]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::values::attitude::Attitude;

    #[test]
    fn friends_that_share_vision_form_one_group_and_vision_off_splits_them() {
        // Five teams: 0 and 3 friends, 3 and 4 friends, so 0, 3 and 4 see as one; 1 and 2
        // friends with vision off see apart.
        let team = Team::new;
        let mut relations = Relations::default();
        relations.set(team(0), team(3), Attitude::Friendly, true);
        relations.set(team(4), team(3), Attitude::Friendly, true);
        relations.set(team(1), team(2), Attitude::Friendly, false);
        relations.set(team(0), team(1), Attitude::Neutral, true);
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
    }
}
