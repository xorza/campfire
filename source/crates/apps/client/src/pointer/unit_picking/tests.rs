use std::f32::consts::FRAC_PI_2;

use bevy::asset::{AssetApp, Handle};
use bevy::camera::PerspectiveProjection;
use bevy::ecs::system::RunSystemOnce;
use bevy::ecs::world::World;
use bevy::math::{Dir3, Quat, Vec2};
use bevy::mesh::{MeshBuilder, Meshable};
use bevy::pbr::{ExtendedMaterial, StandardMaterial};
use bevy::picking::hover::HoverMap;
use bevy::shape::{Cuboid, Ray3d};
use bevy::transform::components::Transform;
use bevy::ui::{Node, Val};
use bevy::window::{CursorMoved, PrimaryWindow, Window, WindowEvent, WindowResolution};
use campfire_client_files::PixelRadius;
use campfire_sim::IdAllocator;
use campfire_test_packages::TestMode;
use campfire_units::UnitTypeBook;
use campfire_values::{EngineTag, FilterData, TagProperties};

use super::*;
use crate::pointer::Pointer;
use crate::test_app;
use crate::view::file_material::{Factors, FileBlend};
use crate::view::footing::Footing;

/// A shape 1 m tall, of `radius`.
const fn post(radius: f32) -> UnitShape {
    UnitShape {
        radius,
        height: 1.0,
        footing: Footing::Circle(radius),
    }
}

/// The file material of `factors` and alpha test `cutoff`.
fn blend(factors: Factors, cutoff: f32) -> FileMaterial {
    ExtendedMaterial {
        base: StandardMaterial::default(),
        extension: FileBlend {
            cutoff,
            team: 0,
            colors: Handle::default(),
            factors,
            depth_write: false,
        },
    }
}

#[test]
fn the_hits_rank_by_tier_then_depth_then_id_by_triangles_and_a_least_radius() {
    // A camera at (0, 0.5, 10) facing −z, of a view 90° high on a screen 720 pixels high: at a
    // depth of 10 m a pixel spans 2 · 10 · tan 45° / 720 = 1/36 m. Each unit is drawn by a cube
    // of side 1 standing on its point, scaled and turned as given.
    let eye = Transform::from_xyz(0.0, 0.5, 10.0);
    let fov = Projection::Perspective(PerspectiveProjection {
        fov: FRAC_PI_2,
        ..PerspectiveProjection::default()
    });
    let scale = PixelScale::of(eye, &fov, 720.0);
    let mut meshes = Assets::<Mesh>::default();
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0).mesh().build()).id();
    let mut materials = Assets::<FileMaterial>::default();
    let beam = materials.add(blend(Factors::Add, 0.0)).id();
    let leaves = materials.add(blend(Factors::Alpha, 0.5)).id();
    let mut world = World::new();
    let mut ids = IdAllocator::default();
    let mut scene = PickScene::default();
    let mut add = |scene: &mut PickScene, tier, at: Vec3, size: Vec3, turn, material, least| {
        let place = Transform::from_translation(at + Vec3::Y * (size.y / 2.0))
            .with_rotation(Quat::from_rotation_y(turn))
            .with_scale(size);
        let target = Target {
            root: world.spawn_empty().id(),
            unit: Pointed {
                id: ids.allocate(),
                team: Team::new(0),
                owner: None,
            },
            tier,
            base: at,
            shape: post(size.x.min(size.z) / 2.0),
            least,
        };
        let part = Part {
            to_local: GlobalTransform::from(place).affine().inverse(),
            center: Vec3::ZERO,
            half: Vec3::splat(0.5),
            mesh: cube,
            material,
        };
        scene.push(target, [part]);
        target.unit.id
    };
    // Along the level ray from (0, 0.5, 10) to −z: a cube of tier 1 at the origin, met on its
    // face z = 0.5, 9.5 along; two of tier 0 at z = −5, 14.5 along, the older of the lower id first; one of
    // tier 0 at z = −10, 19.5 along, of a material that blends with an alpha test, as leaves; one
    // at z = 3 of a material that blends with none, as a headlight's beam, which lets the ray
    // through; and one 8 m long at x = 3, turned a quarter, so it spans x 2.5 to 3.5 and is
    // missed, where unturned, spanning x −1 to 7, it is met on its face z = 0.5.
    let unit = Vec3::ONE;
    let long = Vec3::new(8.0, 1.0, 1.0);
    let near = add(&mut scene, 1, Vec3::ZERO, unit, 0.0, None, 0.0);
    let older = add(&mut scene, 0, Vec3::NEG_Z * 5.0, unit, 0.0, None, 0.0);
    let newer = add(&mut scene, 0, Vec3::NEG_Z * 5.0, unit, 0.0, None, 0.0);
    let far = add(
        &mut scene,
        0,
        Vec3::NEG_Z * 10.0,
        unit,
        0.0,
        Some(leaves),
        0.0,
    );
    add(&mut scene, 0, Vec3::Z * 3.0, unit, 0.0, Some(beam), 0.0);
    add(&mut scene, 0, Vec3::X * 3.0, long, FRAC_PI_2, None, 0.0);
    let assets = PartAssets {
        meshes: &meshes,
        materials: &materials,
    };
    let ray = Ray3d::new(Vec3::new(0.0, 0.5, 10.0), Dir3::NEG_Z);
    let mut hits = Vec::new();
    scene.hits(ray, assets, &mut hits);
    let ranked = |hits: &[UnitHit]| -> Vec<_> {
        hits.iter()
            .map(|hit| (hit.unit.id, hit.tier, hit.depth))
            .collect()
    };
    // The cubes of tier 0 come above the nearer one of tier 1, the lower id first at one depth.
    assert_eq!(
        ranked(&hits),
        [
            (older, 0, 14.5),
            (newer, 0, 14.5),
            (far, 0, 19.5),
            (near, 1, 9.5)
        ]
    );
    scene.clear();
    let unturned = add(&mut scene, 0, Vec3::X * 3.0, long, 0.0, None, 0.0);
    scene.hits(ray, assets, &mut hits);
    assert_eq!(ranked(&hits), [(unturned, 0, 9.5)]);

    // A cube of side 0.2 at the origin, its center at a depth of 10 m, and a level ray 0.2 m to
    // its side: missed by its triangles, met when 10 pixels widen it to 10/36 = 5/18 m round its
    // axis, where x = 0.2, z = √((5/18)² − 0.2²) = √301 / 90, 10 − √301 / 90 along.
    let beside = Ray3d::new(Vec3::new(0.2, 0.5, 10.0), Dir3::NEG_Z);
    scene.clear();
    add(&mut scene, 0, Vec3::ZERO, Vec3::splat(0.2), 0.0, None, 0.0);
    scene.hits(beside, assets, &mut hits);
    assert_eq!(ranked(&hits), []);
    let picking = UnitPicking {
        least_radius_px: 10.0,
        ..UnitPicking::default()
    };
    let least = picking.least_at(&scale, Vec3::Y * 0.5);
    assert!((least - 5.0 / 18.0).abs() < 1e-6, "{least}");
    scene.clear();
    let thin = add(
        &mut scene,
        0,
        Vec3::ZERO,
        Vec3::splat(0.2),
        0.0,
        None,
        least,
    );
    scene.hits(beside, assets, &mut hits);
    let want = 10.0 - 301.0_f32.sqrt() / 90.0;
    assert!(
        hits.len() == 1 && hits[0].unit.id == thin && (hits[0].depth - want).abs() < 1e-5,
        "{hits:?}"
    );
}

#[test]
fn the_mode_s_tiers_rank_a_unit_by_its_type_s_tags_and_its_team_s_relation() {
    // The engine's tags `avatar` and `projectile`, which a filter selects only when it names
    // it, and the mode's `structure`.
    let mut tags = TagNames::new(&[
        EngineTag::new(0, "avatar", TagProperties::NONE),
        EngineTag::new(1, "projectile", TagProperties::NONE.with_named_only()),
    ]);
    let [avatar, projectile] = ["avatar", "projectile"].map(|name| tags.named(name).unwrap());
    let structure = tags.names_mut().add("structure");
    let of = |tiers: &[&str]| {
        let file = PickingFile {
            tiers: (tiers.iter())
                .map(|text| FilterData::parse(text).unwrap())
                .collect(),
            least_radius_px: PixelRadius::new(10.0).unwrap(),
        };
        UnitPicking::resolve(&file, &tags)
    };
    let hero = TagSet::NONE.with(avatar);
    let [hostile, neutral, friendly] = [Relation::Hostile, Relation::Neutral, Relation::Friendly];
    // A hero's player points at enemy heroes, then at enemies, then at the rest: a neutral
    // unit is an enemy, as it may be attacked. With no side, `enemies` selects no unit, as it
    // does not select a friendly one.
    let moba = of(&["enemies:avatar", "enemies"]).unwrap();
    assert_eq!((moba.tiers(), moba.least_radius_px), (3, 10.0));
    let tier = |relation, tags| moba.tier(relation, tags);
    assert_eq!(
        [
            tier(Some(hostile), hero),
            tier(Some(neutral), hero),
            tier(Some(hostile), TagSet::NONE),
            tier(Some(friendly), hero),
            tier(None, hero),
        ],
        [0, 0, 1, 2, 2]
    );
    // Zero Hour's units come before its structures, whatever their relation, and a projectile,
    // which `all` does not select unless it names it, falls to the last tier.
    let zero_hour = of(&["all:!structure"]).unwrap();
    let tier = |relation, tags| zero_hour.tier(relation, tags);
    assert_eq!(
        [
            tier(None, TagSet::NONE),
            tier(Some(friendly), TagSet::NONE),
            tier(None, TagSet::NONE.with(structure)),
            tier(Some(hostile), TagSet::NONE.with(projectile)),
        ],
        [0, 0, 1, 1]
    );
    // A mode of no file: one tier, and no least size. A tag the match lacks refuses the file.
    let none = UnitPicking::default();
    assert_eq!(
        (none.tier(None, hero), none.tiers(), none.least_radius_px),
        (0, 1, 0.0)
    );
    let refused = of(&["enemies", "enemies:flying"]).unwrap_err();
    assert_eq!(refused.to_string(), "no filter \"enemies:flying\"");
    // The MOBA test content's file, against its match's tags: its heroes, then its enemies.
    let packages = TestMode::Moba3v3.packages();
    let books = packages.name_books();
    let tags = books.get::<UnitTypeBook>().unwrap().tags();
    let moba = UnitPicking::resolve(&ClientData::read(&packages).picking, tags).unwrap();
    let hero = TagSet::NONE.with(tags.named("avatar").unwrap());
    assert_eq!((moba.tiers(), moba.least_radius_px), (3, 0.0));
    assert_eq!(
        [Some(hostile), Some(friendly)].map(|relation| moba.tier(relation, hero)),
        [0, 2]
    );
}

#[test]
fn each_tier_s_hits_lie_between_the_camera_s_order_and_its_ui_s() {
    // One tier, a quarter above the camera's order; of two, a third and a sixth above, the
    // first higher, each below the UI's half.
    assert_eq!(tier_order(0, 0, 1), 0.25);
    assert_eq!(tier_order(0, 0, 2), 1.0 / 3.0);
    assert_eq!(tier_order(2, 1, 2), 2.0 + 0.5 / 3.0);
}

#[test]
fn the_hover_takes_the_unit_on_top_and_a_ui_node_over_it_hides_it() {
    // A camera at (0, 0.5, 10) facing −z on a window 800 × 600, the cursor at its center: its
    // ray runs level along −z, through a unit drawn by a cube of side 1 at the origin and one
    // behind it at z = −5.
    let mut app = test_app::headless();
    test_app::add_match(&mut app);
    app.add_plugins(UnitPickingPlugin);
    app.init_asset::<FileMaterial>();
    app.insert_resource(TagBook::new(0, [], []));
    let world = app.world_mut();
    let window = world
        .spawn((
            Window {
                resolution: WindowResolution::new(800, 600),
                ..Window::default()
            },
            PrimaryWindow,
        ))
        .id();
    world.spawn((Camera3d::default(), Transform::from_xyz(0.0, 0.5, 10.0)));
    let cube =
        (world.resource_mut::<Assets<Mesh>>()).add(Cuboid::new(1.0, 1.0, 1.0).mesh().build());
    let mut ids = IdAllocator::default();
    let [near, behind] = [0.0, -5.0].map(|z| {
        let id = ids.allocate();
        let unit = world.spawn((id, Team::new(0), UnitType::new(0))).id();
        let root =
            (world.spawn((DrawingOf(unit), Transform::from_xyz(0.0, 0.0, z), post(0.5)))).id();
        world.spawn((
            Drawn,
            Mesh3d(cube.clone()),
            Transform::from_xyz(0.0, 0.5, 0.0),
            ChildOf(root),
        ));
        (id, root)
    });
    app.update();
    app.world_mut()
        .write_message(WindowEvent::CursorMoved(CursorMoved {
            window,
            position: Vec2::new(400.0, 300.0),
            delta: None,
        }));
    app.update();
    let hovered = |app: &App| {
        let map = app.world().resource::<HoverMap>();
        map.get(&PointerId::Mouse)
            .map(|hits| hits.keys().copied().collect::<Vec<_>>())
    };
    // The post in front blocks the one behind; a query that skips it takes the one behind.
    assert_eq!(hovered(&app), Some(vec![near.1]));
    let under = |app: &mut App, except: Vec<StableId>| {
        (app.world_mut())
            .run_system_once(move |pointer: Pointer<'_, '_>| {
                pointer.unit_under(&except).map(|unit| unit.id)
            })
            .unwrap()
    };
    assert_eq!(under(&mut app, vec![]), Some(near.0));
    assert_eq!(under(&mut app, vec![near.0]), Some(behind.0));
    // A node of the UI over the whole window takes the hover from both.
    let node = (app.world_mut())
        .spawn(Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..Node::default()
        })
        .id();
    app.update();
    app.update();
    assert_eq!(hovered(&app), Some(vec![node]));
}
