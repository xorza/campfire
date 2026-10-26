use campfire_common::{DeclaredName, Tick};
use campfire_geometry::{Bounds, Metric};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};
use std::time::Duration;

use bevy::animation::AnimationClip;
use bevy::animation::graph::AnimationGraph;
use bevy::asset::AssetApp;
use bevy::camera::primitives::MeshAabb;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::world::World;
use bevy::math::Vec3A;
use bevy::time::TimeUpdateStrategy;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot};
use campfire_actions::ShotDamage;
use campfire_areas::{AreaSpec, Areas};
use campfire_capabilities::internals::AreasFixtures;
use campfire_client_files::{
    ClientModel, ClientUnit, ClientUnits, LookDraw, LookVariation, ModelScale, ModelTop,
};
use campfire_declarations::{ById, ByIdBuilder};
use campfire_math::Num;
use campfire_projectiles::{Flight, Payload};
use campfire_script::MatchNames;
use campfire_sim::IdAllocator;
use campfire_units::{
    BodyForm, BodyShape, HeightGrid, Layer, TypeOrigin, TypeOriginBook, Variations,
};
use campfire_values::{ActionId, DamageKind, PackageIndex, PackagePath, Rank};

use super::*;
use crate::test_app;
use crate::view::area_view::{AREA_LIFT, AreaViewPlugin};
use crate::view::look_states::LookStatesPlugin;
use crate::view::map_metric::MapMetric;
use crate::view::unit_looks::{LookNodes, LookTypes};

/// A unit type the client data draws with no model, as every type of a view with no looks: its
/// units take the figure of their team.
const UNDRAWN: UnitType = UnitType::new(0);

/// A view with no window and an empty selection, whose clock moves only as a test sets it.
fn view() -> App {
    view_of(|_| {})
}

/// The view `view` gives, which `setup` gives what it adds before the view's first frame.
fn view_of(setup: impl FnOnce(&mut App)) -> App {
    let mut app = test_app::view();
    app.add_plugins(ViewPlugin);
    setup(&mut app);
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
    // The client's own unit wears its own color, the received one its team's.
    let palette = world.resource::<Palette>();
    assert_eq!(figure(world, predicted).0, palette.own);
    assert_eq!(figure(world, received).0, palette.teams[0]);
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
    // The unit is the client's own, as only those roll back.
    let (alive, standing) = figure(app.world(), unit);
    let dead = app.world().resource::<Palette>().dead.clone();
    assert_eq!(alive, app.world().resource::<Palette>().own);
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
fn a_map_of_no_terrain_is_drawn_on_a_plane_over_its_bounds() {
    // Bounds from (−10, −4) to (30, 6): a plane 40 by 10 m, centered at (10, 1).
    let bounds = Bounds::new([Num::int(-10), Num::int(-4)], [Num::int(30), Num::int(6)]).unwrap();
    let mut app = view_of(|app| {
        app.insert_resource(MapBook {
            bounds,
            ..MapBook::default()
        });
    });
    let world = app.world_mut();
    let mut ground = world.query_filtered::<(&Mesh3d, &Transform), With<Ground>>();
    let (mesh, place) = ground.single(world).unwrap();
    assert_eq!(place.translation, Vec3::new(10.0, 0.0, 1.0));
    let aabb = world
        .resource::<Assets<Mesh>>()
        .get(&mesh.0)
        .unwrap()
        .get_aabb()
        .unwrap();
    assert_eq!(aabb.half_extents, Vec3A::new(20.0, 0.0, 5.0));
}

#[test]
fn the_end_shows_on_the_ground_and_in_the_title_once_it_reaches_the_matchs_entity() {
    // A client that plays on no team, as it waits for its offer: a draw turns the view's own
    // ground gray, and names the end in the window's title, in the frame the end arrives,
    // whatever frames follow.
    let mut app = view();
    let window = app.world_mut().spawn(Window::default()).id();
    let ground = |app: &mut App| {
        let world = app.world_mut();
        let mut ground = world.query_filtered::<&MeshMaterial3d<StandardMaterial>, With<Ground>>();
        let material = ground.single(world).unwrap().0.clone();
        world
            .resource::<Assets<StandardMaterial>>()
            .get(&material)
            .unwrap()
            .base_color
    };
    let before = ground(&mut app);
    app.update();
    assert_eq!(ground(&mut app), before);
    let end = MatchEnd::new(Tick::new(9), MatchResult::Draw);
    app.world_mut().spawn(end);
    app.update();
    assert_eq!(ground(&mut app), Color::srgb(0.35, 0.35, 0.35));
    let title = &app.world().get::<Window>(window).unwrap().title;
    assert_eq!(title, "Campfire: the match ended");
}

#[test]
fn a_position_stands_on_the_drawn_ground_of_a_planar_map_and_is_its_own_point_on_a_spatial_one() {
    // Ground 8 samples high everywhere, a step of 0.5 m: 4 m under every point. A position 2 m
    // up at (3, 0) draws at 4 + 2 = 6 m on a planar map, and at its own 2 m on a spatial one.
    let grid = HeightGrid::new(
        [Num::int(-10), Num::int(-10)],
        Num::int(10),
        Num::from_bits(1 << (Num::FRAC_BITS - 1)),
        4,
        vec![8; 16],
    )
    .unwrap();
    let heights = GroundHeights::of(grid, None);
    let position = Position::from((3, 2, 0));
    let planar = MapMetric(Metric::Planar).lifts(Some(&heights));
    assert_eq!(ground(position, planar), Vec3::new(3.0, 6.0, 0.0));
    let spatial = MapMetric(Metric::Spatial).lifts(Some(&heights));
    assert_eq!(ground(position, spatial), Vec3::new(3.0, 2.0, 0.0));
}

#[test]
fn a_unit_of_a_type_with_models_is_drawn_by_them_turned_its_way_at_its_height() {
    let mut app = view();
    app.add_plugins(LookStatesPlugin);
    app.init_asset::<AnimationClip>();
    app.init_asset::<AnimationGraph>();
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
    // The tank's model is 2.5 m tall; the pit's is drawn under its origin; the shed's file gives
    // no top.
    let model = |name: &str, hide: &[&str], top: Option<f32>| ClientModel {
        model: PackagePath::parse(&format!("client/models/{name}.glb")).unwrap(),
        scale: ModelScale::new(1.25).unwrap(),
        hide: hide.iter().map(|&node| node.to_owned()).collect(),
        top: top.and_then(ModelTop::new),
        upgrades: Vec::new(),
    };
    let names = ["tank", "sound", "pit", "shed", "technical"];
    let models = [
        vec![model("tank", &["tank.flash"], Some(2.5))],
        Vec::new(),
        vec![model("pit", &[], Some(-1.0))],
        vec![model("shed", &[], None)],
        vec![model("technical", &[], Some(1.0))],
    ];
    // The technical's two variations, the second's model 1.5 m tall.
    let variation = |name: &str, top: f32| LookVariation {
        draws: vec![LookDraw::still(vec![model(name, &[], Some(top))])],
        turret: None,
        turret_pitch: None,
    };
    let mut units: BTreeMap<_, _> = names
        .into_iter()
        .zip(models)
        .map(|(name, models)| (DeclaredName::new(name).unwrap(), ClientUnit::still(models)))
        .collect();
    let technical = units.get_mut("technical").unwrap();
    technical.variations = vec![variation("chassis_one", 0.5), variation("chassis_two", 1.5)];
    let looks = ClientUnits::new(units, BTreeMap::new(), BTreeMap::new()).unwrap();
    let data = ClientLoad::of_looks([looks]);
    app.insert_resource(GroundHeights::of(grid, None));
    let origins: TypeOriginBook = names
        .map(|name| TypeOrigin {
            package: PackageIndex::MODE,
            name: name.into(),
        })
        .into_iter()
        .collect();
    let technical_type = origins.iter().nth(4).unwrap().0;
    let form = BodyForm::new(BodyShape::circle(Num::ONE).unwrap(), Layer::FIRST);
    let mut variations = ByIdBuilder::default();
    variations.set(technical_type, Variations::from([form, form]));
    let variations = variations.build();
    let names = MatchNames::default();
    let types = LookTypes {
        origins: &origins,
        variations: Some(&variations),
        names: &names,
    };
    let looks = UnitLooks::of(types, &data);
    app.insert_resource(looks);
    app.insert_resource(data);
    let [unit_type, sound_type, pit_type, shed_type] =
        [0, 1, 2, 3].map(|at| origins.iter().nth(at).unwrap().0);
    // The tank stands 2 m above the ground at (3, 0), turned 90°; a sound beside it.
    let position = Position::from((3, 2, 0));
    let mut ids = IdAllocator::default();
    let unit = app
        .world_mut()
        .spawn((
            ids.allocate(),
            BodyUnit,
            position,
            Team::new(0),
            unit_type,
            Facing::of(Num::int(90)),
        ))
        .id();
    let [sound, pit, shed] = [sound_type, pit_type, shed_type].map(|unit_type| {
        app.world_mut()
            .spawn((
                ids.allocate(),
                BodyUnit,
                Position::from((5, 0, 0)),
                Team::new(0),
                unit_type,
            ))
            .id()
    });
    // A technical of the second variation beside them.
    let technical = app
        .world_mut()
        .spawn((
            ids.allocate(),
            BodyUnit,
            Position::from((7, 0, 0)),
            Team::new(0),
            technical_type,
            Variation::new(1),
        ))
        .id();
    app.update();
    let world = app.world();
    // The sound's look has no models: its drawing stands where it does, of no shape, figure or
    // model, so nothing draws it and nothing points at it.
    let (quiet, _, at) = root(world, sound);
    assert_eq!(at, Vec3::new(5.0, 4.0, 0.0));
    assert!(world.get::<UnitShape>(quiet).is_none() && world.get::<Look>(quiet).is_none());
    assert!(world.get::<Children>(quiet).is_none());
    // Its root stands at the ground's 4 m and its own 2 m, turned a quarter about y, with its
    // shape and no figure, and turns as its unit faces; its one child is its model, at its scale
    // of 1.25, with the node its look hides, painted by its team.
    let (root, _, translation) = root(world, unit);
    assert_eq!(translation, Vec3::new(3.0, 6.0, 0.0));
    assert!(world.get::<UnitShape>(root).is_some() && world.get::<Look>(root).is_none());
    assert!(world.get::<Turns>(root).is_some());
    let turn = Quat::from_rotation_y(90.0_f32.to_radians());
    let turned = world.get::<Transform>(root).unwrap().rotation;
    assert!(turned.abs_diff_eq(turn, 1e-6), "{turned}");
    let children = world.get::<Children>(root).unwrap();
    assert_eq!(children.len(), 1);
    let model = world.entity(children[0]);
    assert!(model.get::<WorldAssetRoot>().is_some());
    assert_eq!(
        model.get::<LookNodes>(),
        Some(&LookNodes {
            hide: ["tank.flash".to_owned()].into(),
            turret: None,
            turret_pitch: None,
        })
    );
    let transform = model.get::<Transform>().unwrap();
    assert_eq!(transform.rotation, Quat::IDENTITY);
    assert_eq!(transform.scale, Vec3::splat(1.25));
    assert_eq!(model.get::<Paint>(), Some(&Paint::of(Team::new(0))));

    // A drawing of models stands as tall as their top as the file gives it, from its first
    // frame: the tank 2.5 m; the pit, drawn under its origin, 0; the shed, of no top, as a
    // structure of no walk and no avatar, 3.8 m.
    let height = |unit| {
        let root = world.get::<Drawing>(unit).unwrap().root();
        world.get::<UnitShape>(root).unwrap().height
    };
    assert_eq!([unit, pit, shed].map(height), [2.5, 0.0, 3.8]);

    // The technical draws its second variation's model, as tall as its 1.5 m, not its look's
    // own (02-engine-core.md, A look varies by its unit's variation).
    let drawn = world.get::<Drawing>(technical).unwrap().root();
    let model = world.get::<Children>(drawn).unwrap()[0];
    let scene = &world.get::<WorldAssetRoot>(model).unwrap().0;
    let path = scene.path().unwrap().to_string();
    assert_eq!(path, "package://0/client/models/chassis_two.glb#Scene0");
    assert_eq!(height(technical), 1.5);
}

#[test]
fn a_projectile_is_drawn_at_its_height_by_its_models_turned_along_its_step_or_as_a_ball() {
    let mut app = view();
    app.init_asset::<WorldAsset>();
    let shell = ClientUnit::still(vec![ClientModel {
        model: PackagePath::parse("client/models/shell.glb").unwrap(),
        scale: ModelScale::new(2.0).unwrap(),
        hide: Vec::new(),
        top: None,
        upgrades: Vec::new(),
    }]);
    let units = BTreeMap::from([(DeclaredName::new("shell").unwrap(), shell)]);
    let data =
        ClientLoad::of_looks([ClientUnits::new(units, BTreeMap::new(), BTreeMap::new()).unwrap()]);
    let origins: TypeOriginBook = ["shell", "bolt"]
        .map(|name| TypeOrigin {
            package: PackageIndex::MODE,
            name: name.into(),
        })
        .into_iter()
        .collect();
    let names = MatchNames::default();
    let types = LookTypes {
        origins: &origins,
        variations: None,
        names: &names,
    };
    let looks = UnitLooks::of(types, &data);
    app.insert_resource(looks);
    app.insert_resource(data);
    let [(shell_type, _), (bolt_type, _)] = [0, 1].map(|at| origins.iter().nth(at).unwrap());
    let mut ids = IdAllocator::default();
    let source = ids.allocate();
    let flight = Flight::Homing { target: source };
    let payload = Payload::Attack(ShotDamage {
        action: ActionId::new(0),
        amount: Num::ONE,
        outer: None,
        kind: DamageKind::new(0),
        rank: Rank::FIRST,
        roll: Num::ZERO,
    });
    let projectile = || Projectile::new(source, flight, payload).unwrap();
    // A shell 3 m up at (1, 0); a bolt of no look beside it.
    let shell = app
        .world_mut()
        .spawn((
            ids.allocate(),
            Position::from((1, 3, 0)),
            shell_type,
            projectile(),
        ))
        .id();
    let bolt = app
        .world_mut()
        .spawn((
            ids.allocate(),
            Position::from((4, 0, 0)),
            bolt_type,
            projectile(),
        ))
        .id();
    app.update();
    // The shell's root stands at its height, unturned, over its one model at its scale.
    let (shell_root, _, at) = root(app.world(), shell);
    assert_eq!(at, Vec3::new(1.0, 3.0, 0.0));
    let world = app.world();
    assert_eq!(
        world.get::<Transform>(shell_root).unwrap().rotation,
        Quat::IDENTITY
    );
    let children = world.get::<Children>(shell_root).unwrap();
    assert_eq!(children.len(), 1);
    let model = world.entity(children[0]);
    assert!(model.get::<WorldAssetRoot>().is_some());
    assert_eq!(model.get::<Transform>().unwrap().scale, Vec3::splat(2.0));
    // It steps 3 m along +x and 3 m down, then 4 m along −z on the level: its `+x` turns 45°
    // below the level, then a quarter about the vertical, from `+x` towards `−z`.
    let turn = |app: &mut App, to: (i32, i32, i32)| {
        app.world_mut().entity_mut(shell).insert(Position::from(to));
        app.update();
        app.world().get::<Transform>(shell_root).unwrap().rotation
    };
    let down = turn(&mut app, (4, 0, 0));
    assert!(
        down.abs_diff_eq(Quat::from_rotation_z(-FRAC_PI_4), 1e-6),
        "{down}"
    );
    assert!((down * Vec3::X).abs_diff_eq(Vec3::new(1.0, -1.0, 0.0).normalize(), 1e-6));
    let level = turn(&mut app, (4, 0, -4));
    assert!(
        level.abs_diff_eq(Quat::from_rotation_y(FRAC_PI_2), 1e-6),
        "{level}"
    );
    // A step of no length keeps the turn.
    assert_eq!(turn(&mut app, (4, 0, -4)), level);
    // The bolt is a ball at a chest's height over its place, which turns none.
    let world = app.world();
    let (bolt_root, _, at) = root(world, bolt);
    assert_eq!(at, Vec3::new(4.0, 0.0, 0.0));
    assert!(world.get::<Heading>(bolt_root).is_none());
    let ball = world.get::<Children>(bolt_root).unwrap()[0];
    assert_eq!(
        world.get::<Transform>(ball).unwrap().translation,
        Vec3::Y * SHOT_HEIGHT
    );
}

#[test]
fn an_area_is_drawn_where_it_lies_as_a_disc_of_its_radius() {
    let origins: TypeOriginBook = [TypeOrigin {
        package: PackageIndex::MODE,
        name: "fire_pool".into(),
    }]
    .into_iter()
    .collect();
    let (unit_type, _) = origins.iter().next().unwrap();
    // An area 2.5 m wide at (3, −4), whose reach the view's areas read from the first frame.
    let mut area = None;
    let mut app = view_of(|app| {
        let radius = Num::from_bits(5 << (Num::FRAC_BITS - 1));
        // The client holds the areas' book as the client's areas declare it, and the names the
        // books share.
        app.init_resource::<ById<UnitType, AreaSpec>>();
        app.init_resource::<MatchNames>();
        area = Some(Areas::standing(app.world_mut(), unit_type, radius));
        app.add_plugins(AreaViewPlugin);
    });
    let area = area.unwrap();
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
