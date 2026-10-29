use bevy_ecs::world::World;
use campfire_common::StableNumber;
use campfire_math::Vec3;
use campfire_sim::StableId;

use crate::arrived_short::ArrivedShort;
use crate::destination::{Destination, WalkGoal};
use crate::walk::{Walk, WalkReadOnly};

#[test]
fn a_walk_to_a_goal_it_arrived_short_of_keeps_the_unit_standing_and_one_into_a_unit_names_it() {
    let short = Vec3::from((4, 0, 0));
    let other = Vec3::from((0, 0, 4));
    let mut world = World::new();
    let unit = world
        .spawn((Destination::default(), ArrivedShort::of(Some(short))))
        .id();
    let goal = |world: &World, entity| world.get::<Destination>(entity).unwrap().get();
    let gives_up = |world: &mut World, entity, to| {
        let mut walks = world.query::<WalkReadOnly>();
        walks.get(world, entity).unwrap().gives_up(to)
    };
    // Short of `short`, the unit stands and gives up a walk there, not one to `other`.
    assert!(gives_up(&mut world, unit, short));
    assert!(!gives_up(&mut world, unit, other));
    Walk::go_to(&mut world, unit, Some(short));
    assert_eq!(goal(&world, unit), None);
    // The same rule holds through the query, and a walk anywhere else goes, as a stop does.
    let mut walks = world.query::<Walk>();
    walks.get_mut(&mut world, unit).unwrap().to(Some(short));
    assert_eq!(goal(&world, unit), None);
    assert!(walks.get_mut(&mut world, unit).unwrap().gives_up(short));
    walks.get_mut(&mut world, unit).unwrap().to(Some(other));
    assert_eq!(goal(&world, unit), Some(other));
    assert!(!gives_up(&mut world, unit, short));
    walks.get_mut(&mut world, unit).unwrap().to(None);
    assert_eq!(goal(&world, unit), None);
    // A walk into a unit at `other` goes there and names the unit; one into a unit at `short`
    // stands, as a walk there does; a walk to a point forgets the unit.
    let container = StableId::from(StableNumber::new(7));
    let into = WalkGoal::Into {
        unit: container,
        at: other,
    };
    let destination = |world: &World, entity| *world.get::<Destination>(entity).unwrap();
    walks
        .get_mut(&mut world, unit)
        .unwrap()
        .to_unit(container, other);
    assert_eq!(destination(&world, unit).goal(), Some(into));
    assert_eq!(goal(&world, unit), Some(other));
    assert_eq!(destination(&world, unit).unit(), Some(container));
    walks.get_mut(&mut world, unit).unwrap().to(None);
    walks
        .get_mut(&mut world, unit)
        .unwrap()
        .to_unit(container, short);
    assert_eq!(goal(&world, unit), None);
    walks
        .get_mut(&mut world, unit)
        .unwrap()
        .to_unit(container, other);
    walks.get_mut(&mut world, unit).unwrap().to(Some(other));
    assert_eq!(
        destination(&world, unit).goal(),
        Some(WalkGoal::Point(other))
    );
    assert_eq!(destination(&world, unit).unit(), None);
}
