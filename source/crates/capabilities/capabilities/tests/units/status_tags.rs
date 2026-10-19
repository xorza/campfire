use campfire_production::Production;
use campfire_units::*;

#[test]
fn a_turn_changes_its_own_tag_alone() {
    let site = StatusTags::from([Production::CONSTRUCTING]);
    let both = StatusTags::from([Production::CONSTRUCTING, Production::GATHERING]);
    assert_eq!(site.turned(Production::GATHERING, true), both);
    assert_eq!(both.turned(Production::GATHERING, false), site);
    assert_eq!(site.turned(Production::CONSTRUCTING, true), site);
    assert_eq!(
        site.turned(Production::CONSTRUCTING, false),
        StatusTags::default()
    );
}
