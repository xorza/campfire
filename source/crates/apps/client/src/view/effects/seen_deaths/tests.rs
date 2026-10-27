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

#[test]
fn a_death_under_way_plays_its_middle_once_its_time_came_and_its_end_as_its_unit_goes() {
    let at = |x: f32| Affine3A::from_translation(Vec3::new(x, 0.0, 0.0));
    let second = Duration::from_secs(1);
    let mut world = World::new();
    let [unit, root] = [world.spawn_empty().id(), world.spawn_empty().id()];
    let mut way = DeathUnderWay {
        death: SeenDeath {
            unit,
            unit_type: UnitType::new(3),
            root,
            place: at(1.0),
            decay: Some(Tick::new(40)),
        },
        middle: Some(second),
    };
    // Dead before its middle, it waits, its place the drawing's latest; at its middle's time it
    // plays the middle, once; then it waits on, where its drawing stood last when its drawing
    // is gone, and plays its end as its unit goes.
    assert_eq!(
        way.step(UnitNow::Dead, Some(at(2.0)), Duration::ZERO),
        DeathStep::Waits
    );
    assert_eq!(way.death.place, at(2.0));
    assert_eq!(way.step(UnitNow::Dead, None, second), DeathStep::Middle);
    assert_eq!(way.step(UnitNow::Dead, None, second * 2), DeathStep::Waits);
    assert_eq!(way.step(UnitNow::Decayed, None, second * 2), DeathStep::End);
    assert_eq!(way.death.place, at(2.0));
    // Its unit despawned at the end of tick 40 went as it decayed when the client holds the state
    // of tick 40 or a later one, which stands before tick 41; before, it left the client's sight,
    // and plays no end, as does a unit of no decay.
    let went = |received: Option<u64>| way.went(received.map(Tick::new));
    assert_eq!(
        [Some(41), Some(45), Some(40), None].map(went),
        [
            UnitNow::Decayed,
            UnitNow::Decayed,
            UnitNow::Left,
            UnitNow::Left
        ]
    );
    assert_eq!(way.step(UnitNow::Left, None, second * 2), DeathStep::Ends);
    let mut staying = way;
    staying.death.decay = None;
    assert_eq!(staying.went(Some(Tick::new(100))), UnitNow::Left);
    // A unit that lives again plays no more of its death.
    way.middle = Some(second);
    assert_eq!(way.step(UnitNow::Living, None, second), DeathStep::Ends);
}
