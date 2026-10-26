use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::system::{In, RunSystemOnce};
use bevy::ecs::world::World;

use super::*;

/// Shows the flash of a shot of the drawing at its root.
fn shot_shows(
    In((root, flash)): In<(Entity, BarrelFlash)>,
    (mut flashes, time): (ResMut<'_, MuzzleFlashes>, Res<'_, Time>),
    (children, names, mut visibilities): (
        Query<'_, '_, &'static Children>,
        Query<'_, '_, &'static Name>,
        Query<'_, '_, &'static mut Visibility>,
    ),
) {
    let mut parts = FlashParts {
        children: &children,
        names: &names,
        visibilities: &mut visibilities,
        now: time.elapsed(),
    };
    flashes.show(&mut parts, root, &flash);
}

#[test]
fn a_shot_shows_its_flash_for_its_time_and_a_later_shot_from_then() {
    // A drawing of two models, each of a hidden flash of the same name, and a hull.
    let mut world = World::new();
    world.insert_resource(Time::<()>::default());
    world.init_resource::<MuzzleFlashes>();
    let root = world.spawn(Name::new("root")).id();
    let flash = |world: &mut World, parent| {
        let flash = (Name::new("tank.flash"), Visibility::Hidden, ChildOf(parent));
        world.spawn(flash).id()
    };
    let [one, two] = [(); 2].map(|()| world.spawn(ChildOf(root)).id());
    let flashes = [flash(&mut world, one), flash(&mut world, two)];
    let hull = (Name::new("tank.hull"), Visibility::Hidden, ChildOf(one));
    let hull = world.spawn(hull).id();
    let shown = BarrelFlash {
        node: "tank.flash".to_owned(),
        ms: 100,
    };
    let show = |world: &mut World| {
        let shot = (root, shown.clone());
        world.run_system_once_with(shot_shows, shot).unwrap();
    };
    let at = |world: &mut World, ms| {
        let mut time = world.resource_mut::<Time>();
        let past = Duration::from_millis(ms) - time.elapsed();
        time.advance_by(past);
        world.run_system_once(MuzzleFlashes::hide_ended).unwrap();
        let seen = |node| world.get::<Visibility>(node).copied();
        [seen(flashes[0]), seen(flashes[1]), seen(hull)]
    };
    let (on, off) = (Some(Visibility::Inherited), Some(Visibility::Hidden));
    // A shot at 0 ms shows both flashes, and the hull stays hidden; they hide at 100 ms.
    show(&mut world);
    assert_eq!(at(&mut world, 99), [on, on, off]);
    assert_eq!(at(&mut world, 100), [off, off, off]);
    // A shot at 100 ms, and one at 150 ms, which shows them from then, until 250 ms.
    show(&mut world);
    assert_eq!(at(&mut world, 150), [on, on, off]);
    show(&mut world);
    assert_eq!(at(&mut world, 249), [on, on, off]);
    // The second model goes; at 250 ms the first's flash hides, and none is left.
    world.despawn(two);
    assert_eq!(at(&mut world, 250), [off, None, off]);
    assert_eq!(world.resource::<MuzzleFlashes>().shown, []);
}
