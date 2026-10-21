use std::f32::consts::FRAC_PI_2;
use std::path::Path;

use bevy::asset::Assets;
use bevy::asset::io::memory::{Dir, MemoryAssetReader};
use bevy::asset::io::{AssetSourceBuilder, AssetSourceId};
use bevy::asset::{AssetApp, AssetPlugin, LoadState};
use bevy::ecs::world::World;
use bevy::gltf::{Gltf, GltfPlugin};
use bevy::math::Vec3;
use bevy::mesh::MeshPlugin;
use bevy::mesh::MeshTag;
use bevy::world_serialization::{WorldAssetRoot, WorldSerializationPlugin};
use campfire_app_threads::OneThreadPlugins;
use campfire_client_files::{
    ClientModel, ClientObjects, ClientUnit, ClientUnits, ModelScale, TeamColors,
};
use campfire_math::Num;
use campfire_units::HeightGrid;
use campfire_values::PackagePath;

use super::*;
use crate::test_app;
use crate::view::file_material::Factors;

/// Two materials as the import writes them, each rough, not metallic and of a specular factor
/// of 0, the second unlit and of extras: an additive blend, no depth write, 2 up; and a
/// triangle of each, whose scene holds its material.
const MATERIALS: &str = r#"{"asset":{"version":"2.0","generator":"campfire-import"},
"extensionsUsed":["CAMPFIRE_material_extras","KHR_materials_specular","KHR_materials_unlit"],
"scene":0,"scenes":[{"nodes":[0]}],"nodes":[{"mesh":0}],
"meshes":[{"primitives":[{"attributes":{"POSITION":0},"material":1},
{"attributes":{"POSITION":0},"material":0}]}],
"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0.0,0.0,0.0],
"max":[0.0,0.0,0.0]}],"bufferViews":[{"buffer":0,"byteLength":36}],
"buffers":[{"byteLength":36,"uri":"data:application/octet-stream;base64,AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}],
"materials":[
{"name":"m_0","pbrMetallicRoughness":{"baseColorFactor":[1.0,0.5,0.0,1.0],"metallicFactor":0.0,
"roughnessFactor":1.0},"emissiveFactor":[0.0,0.0,0.0],"alphaMode":"OPAQUE","doubleSided":false,
"extensions":{"KHR_materials_specular":{"specularFactor":0.0}}},
{"name":"m_1","pbrMetallicRoughness":{"baseColorFactor":[1.0,1.0,1.0,1.0],"metallicFactor":0.0,
"roughnessFactor":1.0},"emissiveFactor":[0.0,0.0,0.0],"alphaMode":"BLEND","doubleSided":true,
"extensions":{"KHR_materials_specular":{"specularFactor":0.0},"KHR_materials_unlit":{},
"CAMPFIRE_material_extras":{"blend":{"mode":"add"},"depth_write":false,"depth_bias":2}}}]}"#;

#[test]
fn a_model_s_meshes_take_its_paint_s_tag_and_follow_its_unit_s_team() {
    // Red for team 0 and blue for team 1: a model of team 0's unit tags its mesh, two levels
    // down, with the first place, 1, and leaves a node of no mesh untagged; its unit moved to
    // team 1, the second place.
    let mut app = App::new();
    let teams = TeamColors::new(vec![[255, 0, 0], [0, 0, 255]]).unwrap();
    app.insert_resource(MatchColors::new(teams));
    app.add_systems(
        Update,
        (UnitModelsPlugin::follow_teams, UnitModelsPlugin::repaint).chain(),
    );
    let world = app.world_mut();
    let unit = world.spawn(Team::new(0)).id();
    let root = world.spawn(DrawingOf(unit)).id();
    let model = world.spawn((Paint::of(Team::new(0)), ChildOf(root))).id();
    let node = world.spawn(ChildOf(model)).id();
    let mesh = world.spawn((Mesh3d::default(), ChildOf(node))).id();
    app.update();
    let tag = |app: &App, entity| app.world().get::<MeshTag>(entity).map(|tag| tag.value);
    assert_eq!((tag(&app, mesh), tag(&app, node)), (Some(1), None));
    app.world_mut().entity_mut(unit).insert(Team::new(1));
    app.update();
    assert_eq!(tag(&app, mesh), Some(2));
    assert_eq!(
        app.world().get::<Paint>(model),
        Some(&Paint::of(Team::new(1)))
    );
}

#[test]
fn the_imports_materials_load_rough_of_no_reflectance_unlit_and_of_extras_where_they_say() {
    let dir = Dir::default();
    dir.insert_asset_text(Path::new("model.gltf"), MATERIALS);
    let mut app = App::new();
    let reader = MemoryAssetReader { root: dir };
    app.register_asset_source(
        AssetSourceId::Default,
        AssetSourceBuilder::new(move || Box::new(reader.clone())),
    );
    app.add_plugins((
        OneThreadPlugins,
        AssetPlugin::default(),
        WorldSerializationPlugin,
        MeshPlugin,
        GltfPlugin::default(),
    ));
    app.init_asset::<FileMaterial>();
    app.world_mut()
        .resource_mut::<GltfExtensionHandlers>()
        .0
        .write_blocking()
        .push(Box::new(ExtrasHandler));
    app.finish();
    app.cleanup();
    let server = app.world().resource::<AssetServer>().clone();
    let model: Handle<Gltf> = server.load("model.gltf");
    for _ in 0..10_000 {
        app.update();
        match server.load_state(&model) {
            LoadState::Loaded => break,
            LoadState::Failed(error) => panic!("{error}"),
            _ => {}
        }
    }
    let world = app.world();
    let gltf = world.resource::<Assets<Gltf>>().get(&model).unwrap();
    let materials = world.resource::<Assets<GltfMaterial>>();
    // Each rough, not metallic, of a reflectance half its specular factor of 0, which is 0;
    // the second unlit.
    let read: Vec<(f32, f32, f32, bool)> = gltf
        .materials
        .iter()
        .map(|handle| {
            let material = materials.get(handle).unwrap();
            let lighting = (material.metallic, material.perceptual_roughness);
            (lighting.0, lighting.1, material.reflectance, material.unlit)
        })
        .collect();
    assert_eq!(read, [(0.0, 1.0, 0.0, false), (0.0, 1.0, 0.0, true)]);
    // Each has its file material as the model loads, which its triangle draws by: the second
    // of its extras over its glTF material; the first, of no extras, opaque and writing depth
    // as Bevy draws its alpha mode.
    let files = world.resource::<Assets<FileMaterial>>();
    let file = |at: usize| {
        let handle = server.get_handle::<FileMaterial>(format!("model.gltf#Material{at}/file"))?;
        files.get(&handle)
    };
    let plain = file(0).unwrap();
    assert_eq!(
        (plain.extension.factors, plain.extension.depth_write),
        (Factors::Replace, true)
    );
    let flash = file(1).unwrap();
    assert_eq!(
        (flash.extension.factors, flash.extension.depth_write),
        (Factors::Add, false)
    );
    assert_eq!((flash.base.depth_bias, flash.base.unlit), (2.0, true));
}

#[test]
fn each_client_object_stands_still_on_the_ground_turned_and_at_its_models_scales() {
    let mut app = test_app::assets();
    // Ground 8 samples high everywhere, a step of 0.5 m: 4 m under every point.
    let grid = HeightGrid::new(
        [Num::int(-10), Num::int(-10)],
        Num::int(10),
        Num::from_bits(1 << (Num::FRAC_BITS - 1)),
        4,
        vec![8; 16],
    )
    .unwrap();
    let name = |text: &str| DeclaredName::new(text).unwrap();
    let model = |path: &str, scale: f32, hide: &[&str]| ClientModel {
        model: PackagePath::parse(path).unwrap(),
        scale: ModelScale::new(scale).unwrap(),
        hide: hide.iter().map(|&node| node.to_owned()).collect(),
        top: None,
    };
    // An oak of two models, one at 1.5 and one hiding its stump; a bush of one.
    let looks = ClientUnits {
        objects: [
            (
                name("oak"),
                ClientUnit::new(vec![
                    model("client/models/oak.glb", 1.5, &[]),
                    model("client/models/leaves.glb", 1.0, &["leaves.stump"]),
                ]),
            ),
            (
                name("bush"),
                ClientUnit::new(vec![model("client/models/bush.glb", 1.0, &[])]),
            ),
        ]
        .into(),
        ..ClientUnits::default()
    };
    // The oak at (3, −2), 1 m up, turned 90°; the bush at the origin; and a ghost, whose look
    // the package lacks.
    let object = |look: &str, x: i64, z: i64, height: i64, angle: i64| ClientObject {
        look: name(look),
        position: [Num::int(x), Num::int(z)],
        height: Num::int(height),
        angle: Num::int(angle),
    };
    let data = ClientData {
        heights: Some(GroundHeights::of(grid, None)),
        objects: ClientObjects {
            objects: vec![
                object("oak", 3, -2, 1, 90),
                object("bush", 0, 0, 0, 0),
                object("ghost", 1, 1, 0, 0),
            ],
        },
        ..ClientData::of_looks([looks])
    };
    app.insert_resource(data);
    app.add_systems(Startup, UnitModelsPlugin::place_objects);
    app.update();
    let world = app.world_mut();
    let mut roots = world.query_filtered::<(&Transform, &Children), With<StillObject>>();
    let mut placed: Vec<(Transform, Vec<(Transform, LookNodes)>)> = roots
        .iter(world)
        .map(|(transform, children)| {
            let models = children
                .iter()
                .map(|child| {
                    let model = world.entity(*child);
                    assert!(model.get::<WorldAssetRoot>().is_some());
                    (
                        *model.get::<Transform>().unwrap(),
                        model.get::<LookNodes>().unwrap().clone(),
                    )
                })
                .collect();
            (*transform, models)
        })
        .collect();
    placed.sort_by(|a, b| a.0.translation.x.total_cmp(&b.0.translation.x));
    // The bush at the ground's 4 m, unturned; the oak at 4 + 1 = 5 m, turned a quarter about
    // y; each model at its own scale, with the nodes its look hides; the ghost is none.
    let look_nodes = |hide: &[&str]| LookNodes {
        hide: hide.iter().map(|&node| node.to_owned()).collect(),
        turret: None,
    };
    let scaled = |scale: f32| Transform::from_scale(Vec3::splat(scale));
    assert_eq!(placed.len(), 2);
    assert_eq!(placed[0].0, Transform::from_xyz(0.0, 4.0, 0.0));
    assert_eq!(placed[0].1, [(scaled(1.0), look_nodes(&[]))]);
    let oak = placed[1].0;
    assert_eq!(oak.translation, Vec3::new(3.0, 5.0, -2.0));
    assert!(
        oak.rotation
            .abs_diff_eq(Quat::from_rotation_y(90.0_f32.to_radians()), 1e-6)
    );
    assert_eq!(
        placed[1].1,
        [
            (scaled(1.5), look_nodes(&[])),
            (scaled(1.0), look_nodes(&["leaves.stump"]))
        ]
    );
}

#[test]
fn a_dressing_hides_the_named_nodes_and_finds_the_turret() {
    let mut world = World::new();
    let [turret, flash, hull, other] = [(); 4].map(|()| world.spawn_empty().id());
    // It hides the flash and a decal, and turns the turret.
    let look_nodes = LookNodes {
        hide: ["tank.decal".to_owned(), "tank.flash".to_owned()].into(),
        turret: Some("tank.turret".into()),
    };
    let node = |entity, name| Node { entity, name };
    let nodes = [
        node(turret, Some("tank.turret")),
        node(flash, Some("tank.flash")),
        node(hull, Some("tank.hull")),
        node(other, None),
    ];
    assert_eq!(
        Dressing::of(&look_nodes, nodes.into_iter()),
        Dressing {
            hidden: vec![flash],
            turret: Some(turret),
        }
    );
    // A look of no turret turns none.
    let still = LookNodes {
        turret: None,
        ..look_nodes
    };
    assert_eq!(Dressing::of(&still, nodes.into_iter()).turret, None);
}

#[test]
fn a_turret_node_turns_about_the_models_up_from_its_rest() {
    // A node at rest tilted 10° about x, turned 90°: the tilt, then a quarter turn about up,
    // so its forward along −z points along −x.
    let rest = Quat::from_rotation_x(10_f32.to_radians());
    let node = TurretNode {
        node: Entity::PLACEHOLDER,
        rest,
    };
    let turned = node.rotation(TurretAngle::of(Num::int(90)));
    assert!(turned.abs_diff_eq(Quat::from_rotation_y(FRAC_PI_2) * rest, 1e-6));
    assert!((turned * rest.inverse() * Vec3::NEG_Z).abs_diff_eq(Vec3::NEG_X, 1e-6));
    assert_eq!(node.rotation(TurretAngle::REST), rest);
}
