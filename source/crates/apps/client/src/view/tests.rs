use std::time::Duration;

use bevy::asset::{AssetApp, AssetPlugin, AssetServer};
use bevy::ecs::system::RunSystemOnce;
use bevy::ecs::world::World;
use bevy::tasks::{IoTaskPool, TaskPool};
use bevy::time::{TimePlugin, TimeUpdateStrategy};
use bevy::world_serialization::WorldAsset;
use campfire_areas::Areas;
use campfire_capabilities::internals::AreasFixtures;
use campfire_client_files::{ClientModel, ClientUnit, ClientUnits, HeightGrid, ModelScale};
use campfire_math::Num;
use campfire_sim::IdAllocator;
use campfire_units::{TypeOrigin, TypeOriginBook};
use campfire_values::{DeclaredName, PackageIndex, PackagePath};

use super::*;
use crate::view::unit_looks::ModelParts;

/// A unit type the client data draws with no model, as every type of a view with no looks: its
/// units take the figure of their team.
const UNDRAWN: UnitType = UnitType::new(0);

/// A view with no window and an empty selection, whose clock moves only as a test sets it.
fn view() -> App {
    let mut app = App::new();
    app.add_plugins((TimePlugin, AssetPlugin::default()));
    app.init_asset::<Mesh>();
    app.init_asset::<StandardMaterial>();
    app.init_resource::<EntityIndex>();
    app.init_resource::<Selection>();
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
    Unpredicted::register(app.world_mut());
    app.init_resource::<ClientData>();
    app.add_plugins(View);
    app.update();
    app
}

/// A spot at `point`, turned none, as a unit of no facing stands.
const fn still(point: Vec3) -> Spot {
    Spot { point, yaw: 0.0 }
}

/// The root of `entity`'s drawing, its glide, and where it stands.
fn root(world: &World, entity: Entity) -> (Entity, Glide, Vec3) {
    let root = world.get::<Drawing>(entity).unwrap().root();
    let glide = *world.get::<Glide>(root).unwrap();
    (
        root,
        glide,
        world.get::<Transform>(root).unwrap().translation,
    )
}

/// The figure of `entity`'s drawing: its material, and its pose over the root.
fn figure(world: &World, entity: Entity) -> (Handle<StandardMaterial>, Transform) {
    let root = world.get::<Drawing>(entity).unwrap().root();
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
        .spawn((
            ids.allocate(),
            BodyUnit,
            Position::from((2, 0, 3)),
            team,
            UNDRAWN,
        ))
        .id();
    let received = app
        .world_mut()
        .spawn((
            ids.allocate(),
            BodyUnit,
            Position::from((0, 0, 0)),
            team,
            UNDRAWN,
            Unpredicted,
        ))
        .id();
    app.update();
    let world = app.world();
    let (first, glide, at) = root(world, predicted);
    assert_eq!(
        (glide, at),
        (
            Glide::ticked(still(Vec3::new(2.0, 0.0, 3.0))),
            Vec3::new(2.0, 0.0, 3.0)
        )
    );
    let figure = world.get::<Look>(first).unwrap().figure;
    assert!(world.get::<Children>(first).unwrap().contains(&figure));
    assert!(world.get::<Figure>(figure).is_some());
    assert_eq!(
        root(world, received).1,
        Glide::timed(still(Vec3::ZERO), Duration::ZERO)
    );

    // The predicted unit moves to x = 4 in a tick; the received one's new place, x = 2, arrives.
    // Neither drawing moves in a frame with no time: the predicted one stands at its place
    // after the tick before last, the received one where it starts its glide.
    *app.world_mut().get_mut::<Position>(predicted).unwrap() = Position::from((4, 0, 3));
    app.world_mut().run_schedule(FixedPostUpdate);
    *app.world_mut().get_mut::<Position>(received).unwrap() = Position::from((2, 0, 0));
    app.update();
    let world = app.world();
    let ticked = Glide::Ticked {
        before: still(Vec3::new(2.0, 0.0, 3.0)),
        last: still(Vec3::new(4.0, 0.0, 3.0)),
    };
    assert_eq!(root(world, predicted).1, ticked);
    let timed = Glide::Timed {
        from: still(Vec3::ZERO),
        to: still(Vec3::new(2.0, 0.0, 0.0)),
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

    // The received unit turns to face 90° and stands: its drawing glides from where it stands
    // to that yaw, π/2.
    app.world_mut()
        .entity_mut(received)
        .insert(Facing::of(Num::int(90)));
    app.update();
    let Glide::Timed { from, to, .. } = root(app.world(), received).1 else {
        panic!("a received unit's glide is timed");
    };
    assert_eq!(from.point, Vec3::new(1.0, 0.0, 0.0));
    assert_eq!((to.point, to.yaw), (Vec3::new(2.0, 0.0, 0.0), FRAC_PI_2));

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
        .spawn((
            IdAllocator::default().allocate(),
            BodyUnit,
            Position::from((0, 0, 0)),
            Team::new(1),
            UNDRAWN,
        ))
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
            scale: ModelScale::new(1.25).unwrap(),
            hide: ["tank.flash".to_owned()].into(),
        }],
    };
    let data = ClientData {
        units: vec![ClientUnits {
            units: [(DeclaredName::new("tank").unwrap(), tank)]
                .into_iter()
                .collect(),
            ..ClientUnits::default()
        }],
        heights: Some(GroundHeights::of(grid, None)),
        ..ClientData::default()
    };
    let origins: TypeOriginBook = [TypeOrigin {
        package: PackageIndex::MODE,
        name: "tank".into(),
    }]
    .into_iter()
    .collect();
    let looks = UnitLooks::of(&origins, &data, app.world().resource::<AssetServer>());
    app.insert_resource(looks);
    app.insert_resource(data);
    let (unit_type, _) = origins.iter().next().unwrap();
    // The tank stands 2 m above the ground at (3, 0), turned 90°.
    let pos = Position::from((3, 2, 0));
    let unit = app
        .world_mut()
        .spawn((
            IdAllocator::default().allocate(),
            BodyUnit,
            pos,
            Team::new(0),
            unit_type,
            Facing::of(Num::int(90)),
        ))
        .id();
    app.update();
    let world = app.world();
    // Its root stands at the ground's 4 m and its own 2 m, turned a quarter about y, with its
    // shape and no figure, and turns as its unit faces; its one child is its model, at its scale
    // of 1.25, with the node its look hides.
    let (root, _, translation) = root(world, unit);
    assert_eq!(translation, Vec3::new(3.0, 6.0, 0.0));
    assert!(world.get::<Shape>(root).is_some() && world.get::<Look>(root).is_none());
    assert!(world.get::<Turns>(root).is_some());
    let turn = Quat::from_rotation_y(90.0_f32.to_radians());
    let turned = world.get::<Transform>(root).unwrap().rotation;
    assert!(turned.abs_diff_eq(turn, 1e-6), "{turned}");
    let children = world.get::<Children>(root).unwrap();
    assert_eq!(children.len(), 1);
    let model = world.entity(children[0]);
    assert!(model.get::<WorldAssetRoot>().is_some());
    assert_eq!(
        model.get::<ModelParts>(),
        Some(&ModelParts {
            package: PackageIndex::MODE,
            hide: ["tank.flash".to_owned()].into(),
        })
    );
    let transform = model.get::<Transform>().unwrap();
    assert_eq!(transform.rotation, Quat::IDENTITY);
    assert_eq!(transform.scale, Vec3::splat(1.25));

    // Until its meshes have their bounds, it is as tall as a structure, of no walk and no avatar.
    let model = children[0];
    assert_eq!(world.get::<Shape>(root).unwrap().height, 3.8);
    // Its scene spawns: a hull 1 m up whose bounds reach 0.5 + 1.5 m over their center, 3 m in
    // the model's space; on the hull, a barrel turned a quarter about z, whose 4 m along x then
    // rise, 1 + 4 = 5 m; and a flash its look hides, higher still. At its scale of 1.25, the tank
    // stands 6.25 m tall.
    let bounds = |center: [f32; 3], half: [f32; 3]| Aabb {
        center: Vec3A::from_array(center),
        half_extents: Vec3A::from_array(half),
    };
    let world = app.world_mut();
    let hull = world
        .spawn((
            Transform::from_xyz(0.0, 1.0, 0.0),
            bounds([0.0, 0.5, 0.0], [1.0, 1.5, 1.0]),
            ChildOf(model),
        ))
        .id();
    world.spawn((
        Transform::from_rotation(Quat::from_rotation_z(FRAC_PI_2)),
        bounds([0.0; 3], [4.0, 0.1, 0.1]),
        ChildOf(hull),
    ));
    world.spawn((
        Transform::from_xyz(0.0, 10.0, 0.0),
        bounds([0.0; 3], [1.0; 3]),
        Visibility::Hidden,
        ChildOf(model),
    ));
    app.update();
    let shape = app.world().get::<Shape>(root).unwrap();
    assert!((shape.height - 6.25).abs() < 1e-5, "{}", shape.height);
}

#[test]
fn an_area_is_drawn_where_it_lies_as_a_disc_of_its_radius() {
    let mut app = view();
    let origins: TypeOriginBook = [TypeOrigin {
        package: PackageIndex::MODE,
        name: "fire_pool".into(),
    }]
    .into_iter()
    .collect();
    let (unit_type, _) = origins.iter().next().unwrap();
    // An area 2.5 m wide at (3, −4).
    let area = Areas::standing(
        app.world_mut(),
        unit_type,
        Num::from_bits(5 << (Num::FRAC_BITS - 1)),
    );
    let lies = app
        .world_mut()
        .spawn((
            IdAllocator::default().allocate(),
            Position::from((3, 0, -4)),
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

#[test]
fn a_selected_unit_is_ringed_at_its_reach_once_drawn_until_it_leaves_the_selection() {
    let mut app = view();
    let mut ids = IdAllocator::default();
    let team = Team::new(0);
    let [tower_id, hero_id] = [ids.allocate(), ids.allocate()];
    let tower = app
        .world_mut()
        .spawn((tower_id, BodyUnit, Position::from((0, 0, 0)), team, UNDRAWN))
        .id();
    // The rings under `unit`'s drawing: the scale of each.
    let rings = |world: &World, unit: Entity| -> Vec<Vec3> {
        let ring = &world.resource::<Palette>().ring;
        let root = world.get::<Drawing>(unit).unwrap().root();
        world
            .get::<Children>(root)
            .unwrap()
            .iter()
            .copied()
            .filter(|&child| {
                world
                    .get::<Mesh3d>(child)
                    .is_some_and(|mesh| mesh.0 == *ring)
            })
            .map(|child| {
                let transform = world.get::<Transform>(child).unwrap();
                assert_eq!(transform.translation, Vec3::Y * RING_LIFT);
                transform.scale
            })
            .collect()
    };
    // The tower and the hero, not held yet, selected: the tower, of no owner and no walk, is a
    // structure of reach 0.9 m, ringed as it is drawn.
    app.world_mut()
        .resource_mut::<Selection>()
        .pick(&[tower_id, hero_id]);
    app.update();
    assert_eq!(rings(app.world(), tower), [Vec3::splat(0.9)]);
    // The hero arrives, a player's avatar, of reach 0.5 m: ringed as it is drawn, with no change
    // of the selection.
    let hero = app
        .world_mut()
        .spawn((
            hero_id,
            BodyUnit,
            Position::from((4, 0, 0)),
            team,
            UNDRAWN,
            Avatar,
        ))
        .id();
    app.update();
    assert_eq!(rings(app.world(), hero), [Vec3::splat(0.5)]);
    assert_eq!(rings(app.world(), tower).len(), 1);
    // The hero alone selected: the tower's ring goes, the hero keeps its one.
    app.world_mut().resource_mut::<Selection>().pick(&[hero_id]);
    app.update();
    assert_eq!(rings(app.world(), tower), []);
    assert_eq!(rings(app.world(), hero), [Vec3::splat(0.5)]);
}

#[test]
fn a_model_drawing_stands_as_tall_as_the_top_of_its_shown_bounds() {
    // An axis turned to rise: x along y, y along −x. A box at (2, 5, 0) of half extents
    // (3, 1, 1), its frame 1 m up, reaches 1 + 2 + 3 = 6 m: the half extent along x rises.
    let turned = Affine3A::from_cols(Vec3A::Y, Vec3A::NEG_X, Vec3A::Z, Vec3A::Y);
    let bounds = |center: Vec3, half: Vec3| Aabb::from_min_max(center - half, center + half);
    let wide = bounds(Vec3::new(2.0, 5.0, 0.0), Vec3::new(3.0, 1.0, 1.0));
    assert_eq!(View::bounds_top(turned, &wide), 6.0);

    // A root 2 m tall, its model 1 m up and twice as tall: its mesh, of y from 0 to 1, reaches
    // 1 + 2 · 1 = 3 m; its child 1 m above it in its frame, 1 + 2 · 1 = 3 m, of half extent
    // 0.25, reaches 3 + 2 · 0.25 = 3.5 m. A hidden node 10 m tall counts for nothing, nor does
    // its child.
    let mut world = World::new();
    let shape = Shape {
        radius: 0.5,
        height: 2.0,
        footing: Footing::Circle(0.5),
    };
    let root = world.spawn((shape, FitsModel, Transform::default())).id();
    let doubled = Transform::from_xyz(0.0, 1.0, 0.0).with_scale(Vec3::new(1.0, 2.0, 1.0));
    let model = world
        .spawn((
            doubled,
            bounds(Vec3::Y * 0.5, Vec3::splat(0.5)),
            ChildOf(root),
        ))
        .id();
    let small = bounds(Vec3::ZERO, Vec3::splat(0.25));
    world.spawn((Transform::from_xyz(0.0, 1.0, 0.0), small, ChildOf(model)));
    let tall = bounds(Vec3::ZERO, Vec3::splat(10.0));
    let hidden = world
        .spawn((
            Transform::default(),
            tall,
            Visibility::Hidden,
            ChildOf(root),
        ))
        .id();
    world.spawn((Transform::default(), tall, ChildOf(hidden)));
    // A root whose models have no bounds yet keeps its height, and waits.
    let waiting = world.spawn((shape, FitsModel, Transform::default())).id();
    world.spawn((Transform::default(), ChildOf(waiting)));
    world.run_system_once(View::fit_models).unwrap();
    world.flush();
    assert_eq!(world.get::<Shape>(root).unwrap().height, 3.5);
    assert!(!world.entity(root).contains::<FitsModel>());
    assert_eq!(world.get::<Shape>(waiting).unwrap().height, 2.0);
    assert!(world.entity(waiting).contains::<FitsModel>());
}
