use std::f32::consts::FRAC_PI_2;
use std::time::Duration;

use bevy::asset::{AssetApp, AssetPlugin, AssetServer};
use bevy::ecs::hierarchy::Children;
use bevy::ecs::world::World;
use bevy::tasks::{IoTaskPool, TaskPool};
use bevy::time::{TimePlugin, TimeUpdateStrategy};
use bevy::world_serialization::WorldAsset;
use campfire_capabilities::internals::standing_area;
use campfire_capabilities::{DeclaredName, HeightGrid, PackagePath, TypeOrigin, TypeOrigins};
use campfire_math::Num;
use campfire_package::{ClientModel, ClientUnit, ClientUnits};
use campfire_sim::IdAllocator;

use super::*;
use crate::view::unit_looks::ModelParts;

/// A view with no window, whose clock moves only as a test sets it.
fn view() -> App {
    let mut app = App::new();
    app.add_plugins((TimePlugin, AssetPlugin::default()));
    app.init_asset::<Mesh>();
    app.init_asset::<StandardMaterial>();
    app.init_resource::<EntityIndex>();
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
    Unpredicted::register(app.world_mut());
    app.add_plugins(View);
    app.update();
    app
}

/// A place on the ground, in whole meters.
fn place(x: i64, z: i64) -> Position {
    Position::new(campfire_math::Vec3::new(
        Num::int(x),
        Num::ZERO,
        Num::int(z),
    ))
    .unwrap()
}

/// The root of `unit`'s drawing, its glide, and where it stands.
fn root(world: &World, unit: Entity) -> (Entity, Glide, Vec3) {
    let root = world.get::<Drawing>(unit).unwrap().root();
    let glide = *world.get::<Glide>(root).unwrap();
    (
        root,
        glide,
        world.get::<Transform>(root).unwrap().translation,
    )
}

/// The figure of `unit`'s drawing: its material, and its pose over the root.
fn figure(world: &World, unit: Entity) -> (Handle<StandardMaterial>, Transform) {
    let root = world.get::<Drawing>(unit).unwrap().root();
    let figure = world.get::<Look>(root).unwrap().figure;
    let material = world
        .get::<MeshMaterial3d<StandardMaterial>>(figure)
        .unwrap();
    (material.0.clone(), *world.get::<Transform>(figure).unwrap())
}

#[test]
fn a_drawing_follows_its_units_clock_and_goes_with_it() {
    let mut app = view();
    let mut ids = IdAllocator::default();
    let team = Team::new(0);
    let predicted = app
        .world_mut()
        .spawn((ids.allocate(), place(2, 3), team))
        .id();
    let received = app
        .world_mut()
        .spawn((ids.allocate(), place(0, 0), team, Unpredicted))
        .id();
    app.update();
    let world = app.world();
    let (first, glide, at) = root(world, predicted);
    assert_eq!(
        (glide, at),
        (
            Glide::ticked(Vec3::new(2.0, 0.0, 3.0)),
            Vec3::new(2.0, 0.0, 3.0)
        )
    );
    let figure = world.get::<Look>(first).unwrap().figure;
    assert!(world.get::<Children>(first).unwrap().contains(&figure));
    assert!(world.get::<Figure>(figure).is_some());
    assert_eq!(
        root(world, received).1,
        Glide::timed(Vec3::ZERO, Duration::ZERO)
    );

    // The predicted unit moves to x = 4 in a tick; the received one's new place, x = 2, arrives.
    // Neither drawing moves in a frame with no time: the predicted one stands at its place
    // after the tick before last, the received one where it starts its glide.
    *app.world_mut().get_mut::<Position>(predicted).unwrap() = place(4, 3);
    app.world_mut().run_schedule(FixedPostUpdate);
    *app.world_mut().get_mut::<Position>(received).unwrap() = place(2, 0);
    app.update();
    let world = app.world();
    let ticked = Glide::Ticked {
        before: Vec3::new(2.0, 0.0, 3.0),
        last: Vec3::new(4.0, 0.0, 3.0),
    };
    assert_eq!(root(world, predicted).1, ticked);
    let timed = Glide::Timed {
        from: Vec3::ZERO,
        to: Vec3::new(2.0, 0.0, 0.0),
        since: Duration::ZERO,
    };
    assert_eq!(root(world, received).1, timed);
    assert_eq!(root(world, predicted).2, Vec3::new(2.0, 0.0, 3.0));
    assert_eq!(root(world, received).2, Vec3::ZERO);

    // Half a tick of 1/64 s, 7.8125 ms, exact in binary: both half way, by the fixed clock's
    // share of its next tick and by the time since the glide started.
    let half = app.world().resource::<Time<Fixed>>().timestep() / 2;
    assert_eq!(
        half,
        Duration::from_micros(7_812) + Duration::from_nanos(500)
    );
    app.insert_resource(TimeUpdateStrategy::ManualDuration(half));
    app.update();
    let world = app.world();
    assert_eq!(root(world, predicted).2, Vec3::new(3.0, 0.0, 3.0));
    assert_eq!(root(world, received).2, Vec3::new(1.0, 0.0, 0.0));

    // Gone, a unit takes its whole drawing along.
    app.world_mut().despawn(predicted);
    app.world_mut().despawn(received);
    app.update();
    let world = app.world_mut();
    assert_eq!(world.query::<&DrawingOf>().iter(world).count(), 0);
    assert_eq!(world.query::<&Figure>().iter(world).count(), 0);
}

#[test]
fn a_drawing_shows_its_unit_as_it_stands_after_a_rollback() {
    let mut app = view();
    let unit = app
        .world_mut()
        .spawn((IdAllocator::default().allocate(), place(0, 0), Team::new(1)))
        .id();
    app.update();
    let (alive, standing) = figure(app.world(), unit);
    let dead = app.world().resource::<Palette>().dead.clone();
    assert_eq!(alive, app.world().resource::<Palette>().teams[1]);
    assert_eq!(standing.rotation, Quat::IDENTITY);
    // A structure's capsule of radius 0.9 lies with its axis 0.9 over the ground.
    let lying =
        Transform::from_translation(Vec3::Y * 0.9).with_rotation(Quat::from_rotation_z(FRAC_PI_2));
    app.world_mut().entity_mut(unit).insert(Dead);
    app.update();
    assert_eq!(figure(app.world(), unit), (dead.clone(), lying));
    // A rollback across the death takes `Dead` away and gives it back in one frame: the unit is
    // still dead.
    app.world_mut()
        .entity_mut(unit)
        .remove::<Dead>()
        .insert(Dead);
    app.update();
    assert_eq!(figure(app.world(), unit), (dead, lying));
    app.world_mut().entity_mut(unit).remove::<Dead>();
    app.update();
    assert_eq!(figure(app.world(), unit), (alive, standing));
}

#[test]
fn a_unit_leans_its_top_towards_its_target() {
    // Towards +x: the top, (0, 1, 0), turns by 0.35 rad to (sin 0.35, cos 0.35, 0).
    let from = Vec3::new(1.0, 0.0, 2.0);
    let top = lean_toward(from, Vec3::new(5.0, 0.0, 2.0)) * Vec3::Y;
    let expected = Vec3::new(LEAN.sin(), LEAN.cos(), 0.0);
    // A rotation in f32 rounds each component to some ulps of 1, below 1e-6.
    assert!(top.abs_diff_eq(expected, 1e-6), "{top}");
    // Towards −z, whatever the target's height: the top to (0, cos 0.35, −sin 0.35).
    let top = lean_toward(from, Vec3::new(1.0, 3.0, -4.0)) * Vec3::Y;
    let expected = Vec3::new(0.0, LEAN.cos(), -LEAN.sin());
    assert!(top.abs_diff_eq(expected, 1e-6), "{top}");
    // On one spot, no lean.
    assert_eq!(lean_toward(from, from), Quat::IDENTITY);
}

#[test]
fn the_result_stands_by_the_clients_team() {
    let [a, b] = [0, 1].map(Team::new);
    let won = MatchResult::Won(a);
    assert_eq!(Standing::of(won, Some(a)), Some(Standing::Victory));
    assert_eq!(Standing::of(won, Some(b)), Some(Standing::Defeat));
    assert_eq!(Standing::of(won, None), None);
    for own in [Some(a), None] {
        assert_eq!(Standing::of(MatchResult::Draw, own), Some(Standing::Draw));
    }
}

#[test]
fn a_unit_of_a_type_with_models_is_drawn_by_them_turned_its_way_at_its_height() {
    IoTaskPool::get_or_init(TaskPool::new);
    let mut app = view();
    app.init_asset::<WorldAsset>();
    // Ground 8 samples high everywhere, a step of 0.5 m: 4 m under every point.
    let grid = HeightGrid::new(
        [Num::int(-10), Num::int(-10)],
        Num::int(10),
        Num::from_bits(1 << (Num::FRAC_BITS - 1)),
        4,
        vec![8; 16],
    )
    .unwrap();
    let tank = ClientUnit {
        models: vec![ClientModel {
            model: PackagePath::parse("client/models/tank.glb").unwrap(),
            hide: vec!["tank.flash".to_owned()],
        }],
    };
    let data = ClientData {
        units: vec![ClientUnits {
            units: [(DeclaredName::new("tank").unwrap(), tank)]
                .into_iter()
                .collect(),
        }],
        heights: GroundHeights::of(&grid),
        ..ClientData::default()
    };
    let origins: TypeOrigins = [TypeOrigin {
        package: 0,
        name: "tank".into(),
    }]
    .into_iter()
    .collect();
    let looks = UnitLooks::of(&origins, &data, app.world().resource::<AssetServer>());
    app.insert_resource(looks);
    app.insert_resource(data);
    let (unit_type, _) = origins.iter().next().unwrap();
    // The tank stands 2 m above the ground at (3, 0), turned 90°.
    let at = Position::new(campfire_math::Vec3::new(
        Num::int(3),
        Num::int(2),
        Num::ZERO,
    ))
    .unwrap();
    let unit = app
        .world_mut()
        .spawn((
            IdAllocator::default().allocate(),
            at,
            Team::new(0),
            unit_type,
            Facing::of(Num::int(90)),
        ))
        .id();
    app.update();
    let world = app.world();
    // Its root stands at the ground's 4 m and its own 2 m, with its shape and no figure; its one
    // child is its model, turned a quarter about y, with the node its look hides.
    let (root, _, translation) = root(world, unit);
    assert_eq!(translation, Vec3::new(3.0, 6.0, 0.0));
    assert!(world.get::<Shape>(root).is_some() && world.get::<Look>(root).is_none());
    let children = world.get::<Children>(root).unwrap();
    assert_eq!(children.len(), 1);
    let model = world.entity(children[0]);
    assert!(model.get::<WorldAssetRoot>().is_some());
    assert_eq!(
        model.get::<ModelParts>(),
        Some(&ModelParts {
            package: 0,
            hide: vec!["tank.flash".to_owned()],
        })
    );
    let turn = model.get::<Transform>().unwrap().rotation;
    assert!(turn.abs_diff_eq(Quat::from_rotation_y(90.0_f32.to_radians()), 1e-6));
}

#[test]
fn an_area_is_drawn_where_it_lies_as_a_disc_of_its_radius() {
    let mut app = view();
    let origins: TypeOrigins = [TypeOrigin {
        package: 0,
        name: "fire_pool".into(),
    }]
    .into_iter()
    .collect();
    let (unit_type, _) = origins.iter().next().unwrap();
    // An area 2.5 m wide at (3, −4).
    let area = standing_area(
        app.world_mut(),
        unit_type,
        Num::from_bits(5 << (Num::FRAC_BITS - 1)),
    );
    let lies = app
        .world_mut()
        .spawn((
            IdAllocator::default().allocate(),
            place(3, -4),
            Team::new(0),
            unit_type,
            area,
        ))
        .id();
    app.update();
    let world = app.world();
    let (root, _, at) = root(world, lies);
    assert_eq!(at, Vec3::new(3.0, 0.0, -4.0));
    let disc = world.get::<Children>(root).unwrap()[0];
    let disc = world.get::<Transform>(disc).unwrap();
    assert_eq!(disc.scale, Vec3::new(2.5, 1.0, 2.5));
    assert_eq!(disc.translation, Vec3::Y * AREA_LIFT);
}
