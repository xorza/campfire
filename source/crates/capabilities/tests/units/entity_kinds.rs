use std::any::TypeId;

use bevy_ecs::component::ComponentInfo;
use campfire_capabilities::Match;
use campfire_core::*;
use campfire_sim::HostRule;

#[test]
fn the_unit_group_hosts_exactly_the_kinds_that_require_it() {
    // `UnitEntity`'s host rule lists its kinds by hand; each kind's marker requires it. The
    // two agree: a kind that requires it is in the list, and none other is.
    let game = Match::test(&[], Match::TEST_RATE, None);
    let components = game.components();
    let unit = components.component_id::<UnitEntity>().unwrap();
    let kinds = game.resource::<EntityKinds>();
    let mut requiring: Vec<TypeId> = kinds
        .0
        .iter()
        .filter_map(|&id| components.get_info(id))
        .filter(|info| info.required_components().iter_ids().any(|id| id == unit))
        .filter_map(ComponentInfo::type_id)
        .collect();
    let mut hosted = Vec::new();
    assert!(UnitEntity::kinds(&mut hosted));
    requiring.sort_unstable();
    hosted.sort_unstable();
    assert_eq!(requiring.len(), 2);
    assert_eq!(requiring, hosted);
}
