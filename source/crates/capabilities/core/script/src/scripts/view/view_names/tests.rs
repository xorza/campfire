use std::num::NonZeroU32;
use std::sync::Arc;

use bevy_ecs::world::World;
use campfire_sim::MatchRate;
use campfire_values::{Names, ResourceId};
use campfire_visibility::Visibility;

use super::*;
use crate::scripts::view::View;

#[test]
fn the_view_derives_its_names_from_the_match_names_as_it_reads_once_they_changed() {
    let mut world = World::new();
    let mut names = MatchNames::default();
    names.set_table(Arc::new(Names::<ResourceId>::unique(["gold"])));
    world.insert_resource(names);
    let view = View::new(
        MatchRate::new(NonZeroU32::new(30).unwrap()),
        Visibility::DEFAULT,
        |_| {},
    );
    // Before its first read the view holds no name; the read derives them, scripts' strings
    // among them.
    assert_eq!(view.names().count::<ResourceId>(), 0);
    view.read(&mut world);
    let gold = ResourceId::new(0);
    assert_eq!(view.names().named::<ResourceId>("gold"), Some(gold));
    assert_eq!(ScriptText::from(view.names().name(gold)).as_str(), "gold");
    // A change of the match's names reaches the view at its next read, not before.
    let both = Names::<ResourceId>::unique(["gold", "wood"]);
    world.resource_mut::<MatchNames>().set_table(Arc::new(both));
    assert_eq!(view.names().count::<ResourceId>(), 1);
    view.read(&mut world);
    assert_eq!(view.names().count::<ResourceId>(), 2);
    assert!(world.resource::<MatchNames>().fits_resources(5));
}
