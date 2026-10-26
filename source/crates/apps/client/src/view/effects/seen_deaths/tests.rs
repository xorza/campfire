use bevy::ecs::world::World;
use bevy::math::Vec3;
use bevy::transform::components::Transform;

use super::*;
use crate::view::drawing::DrawingOf;

#[test]
fn a_death_is_seen_as_dead_reaches_its_unit_and_plays_unless_a_rollback_takes_it_back() {
    let mut world = World::new();
    Unpredicted::register(&mut world);
    world.init_resource::<SeenDeaths>();
    world.add_observer(SeenDeaths::see);
    // Three drawn units of type 3, their roots at x = 1, 2 and 3, and one undrawn.
    let unit_type = UnitType::new(3);
    let drawn = [1.0, 2.0, 3.0].map(|x| {
        let unit = world.spawn(unit_type).id();
        let place = GlobalTransform::from(Transform::from_xyz(x, 0.0, 0.0));
        let root = world.spawn((DrawingOf(unit), place)).id();
        (unit, root)
    });
    let [(gone, gone_root), (stays, _), (flickers, _)] = drawn;
    let undrawn = world.spawn(unit_type).id();
    // The first dies and despawns in one frame, as a unit whose death and despawn the client
    // applies together; the second dies and stays; the third gains `Dead` again in a rollback,
    // which took it away; the undrawn one shows no death.
    for unit in [gone, stays, flickers, undrawn] {
        world.entity_mut(unit).insert(Dead);
    }
    world.entity_mut(gone).despawn();
    world.flush();
    let seen: Vec<_> = world
        .resource::<SeenDeaths>()
        .0
        .iter()
        .map(|death| death.unit)
        .collect();
    assert_eq!(seen, [gone, stays, flickers]);
    let first = world.resource::<SeenDeaths>().0[0];
    assert_eq!(
        (first.unit_type, first.root, first.place.translation),
        (unit_type, gone_root, Vec3::new(1.0, 0.0, 0.0).into())
    );
    // The frame's removals name the gone unit and the one the rollback took `Dead` from: the gone
    // one plays, the one that stands does not, and the one that stays plays.
    let mut revived = vec![gone, flickers];
    revived.sort_unstable();
    let mut deaths = std::mem::take(&mut *world.resource_mut::<SeenDeaths>());
    let stands = |unit| world.get_entity(unit).is_ok();
    let played: Vec<_> = deaths
        .played(&revived, stands)
        .map(|death| death.unit)
        .collect();
    assert_eq!(played, [gone, stays]);
    assert!(deaths.0.is_empty());
}
