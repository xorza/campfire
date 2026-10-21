use campfire_values::{Relation, VisionShare};

use super::*;

#[test]
fn friends_that_share_vision_form_one_group_and_vision_off_splits_them() {
    // Five teams: 0 and 3 friends, 3 and 4 friends, so 0, 3 and 4 see as one; 1 and 2
    // friends with vision off see apart.
    let team = Team::new;
    let mut relations = Relations::default();
    relations.set(team(0), team(3), Relation::Friendly, VisionShare::Shared);
    relations.set(team(4), team(3), Relation::Friendly, VisionShare::Shared);
    relations.set(team(1), team(2), Relation::Friendly, VisionShare::Apart);
    relations.set(team(0), team(1), Relation::Neutral, VisionShare::Shared);
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
    relations.set(team(2), team(5), Relation::Friendly, VisionShare::Shared);
    relations.set(team(1), team(5), Relation::Friendly, VisionShare::Shared);
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
