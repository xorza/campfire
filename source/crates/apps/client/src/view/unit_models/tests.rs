use std::f32::consts::FRAC_PI_2;
use std::path::Path;

use bevy::asset::Assets;
use bevy::asset::io::memory::{Dir, MemoryAssetReader};
use bevy::asset::io::{AssetSourceBuilder, AssetSourceId};
use bevy::asset::{Asset, AssetApp, LoadState};
use bevy::ecs::system::In;
use bevy::ecs::world::World;
use bevy::gltf::{Gltf, GltfPlugin};
use bevy::mesh::MeshPlugin;
use bevy::mesh::MeshTag;
use bevy::world_serialization::{WorldAssetRoot, WorldSerializationPlugin};
use campfire_client_files::{
    ClientModel, ClientObjects, ClientUnits, ModelScale, ObjectLook, TeamColors,
};
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

/// An app of Bevy's glTF loader and the import's extras, of `text` at `model.gltf`.
fn gltf_app(text: &str) -> App {
    let dir = Dir::default();
    dir.insert_asset_text(Path::new("model.gltf"), text);
    let reader = MemoryAssetReader { root: dir };
    let source = AssetSourceBuilder::new(move || Box::new(reader.clone()));
    let mut app = test_app::assets_with(AssetSourceId::Default, source);
    app.add_plugins((WorldSerializationPlugin, MeshPlugin, GltfPlugin::default()));
    app.init_asset::<FileMaterial>();
    app.init_asset::<AnimationClip>();
    app.init_asset::<ClipTiming>();
    app.world_mut()
        .resource_mut::<GltfExtensionHandlers>()
        .0
        .write_blocking()
        .push(Box::new(ExtrasHandler));
    app.finish();
    app.cleanup();
    app
}

/// Updates `app` until the asset of `handle` loads.
fn wait<A: Asset>(app: &mut App, handle: &Handle<A>) {
    let server = app.world().resource::<AssetServer>().clone();
    for _ in 0..10_000 {
        app.update();
        match server.load_state(handle) {
            LoadState::Loaded => return,
            LoadState::Failed(error) => panic!("{error}"),
            _ => {}
        }
    }
    panic!("the asset loads");
}

/// An app of Bevy's glTF loader and the import's extras that loaded `text` as `model.gltf`, and
/// the handle of its glTF.
fn loaded(text: &str) -> (App, Handle<Gltf>) {
    let mut app = gltf_app(text);
    let model: Handle<Gltf> = app.world().resource::<AssetServer>().load("model.gltf");
    wait(&mut app, &model);
    (app, model)
}

#[test]
fn a_model_that_replaces_another_shows_as_it_is_dressed_and_a_new_state_hides_again() {
    // A model of a root pivot, its hull, and an arm on it of its flag, the flag hidden.
    let named = MATERIALS.replace(
        r#""scenes":[{"nodes":[0]}],"nodes":[{"mesh":0}],"#,
        concat!(
            r#""scenes":[{"nodes":[0]}],"nodes":[{"name":"root","children":[1,2]},"#,
            r#"{"name":"hull","mesh":0},{"name":"arm","children":[3]},{"name":"flag","mesh":0}],"#,
        ),
    );
    let (mut app, model) = loaded(&named);
    let teams = TeamColors::new(vec![[255, 0, 0]]).unwrap();
    app.insert_resource(MatchColors::new(teams));
    app.add_observer(UnitModelsPlugin::dress);
    app.add_systems(Update, UnitModelsPlugin::redress);
    app.register_type::<MeshMaterial3d<FileMaterial>>();
    let scene = app
        .world()
        .resource::<Assets<Gltf>>()
        .get(&model)
        .unwrap()
        .scenes[0]
        .clone();
    let nodes = |hide: &[&str]| LookNodes {
        hide: hide.iter().map(|&name| name.to_owned()).collect(),
        turret: None,
        turret_pitch: None,
    };
    // A model of the state before, shown, and the one of the new state, hidden as it loads.
    let world = app.world_mut();
    let root = world.spawn(Visibility::default()).id();
    let before = world.spawn(ChildOf(root)).id();
    let after = world
        .spawn((
            WorldAssetRoot(scene),
            nodes(&["flag"]),
            Paint::default(),
            Visibility::Hidden,
            Replaces(before),
            ChildOf(root),
        ))
        .id();
    let node = |app: &App, name: &str| {
        let world = app.world();
        let mut below = vec![after];
        while let Some(node) = below.pop() {
            if world
                .get::<Name>(node)
                .is_some_and(|named| named.as_str() == name)
            {
                return Some(node);
            }
            below.extend(world.get::<Children>(node).into_iter().flatten());
        }
        None
    };
    for _ in 0..100 {
        app.update();
        if node(&app, "flag").is_some() {
            break;
        }
    }
    app.update();
    // Dressed, it shows, and the one before goes; its flag is hidden.
    let world = app.world();
    assert!(world.get_entity(before).is_err());
    assert_eq!(world.get::<Visibility>(after), Some(&Visibility::Inherited));
    assert!(world.get::<Replaces>(after).is_none());
    let [hull, flag] = ["hull", "flag"].map(|name| node(&app, name).unwrap());
    let visibility = |app: &App, node| app.world().get::<Visibility>(node).copied();
    assert_eq!(visibility(&app, flag), Some(Visibility::Hidden));
    // A new state of the same model hides its hull and shows its flag.
    app.world_mut()
        .entity_mut(after)
        .insert((nodes(&["hull"]), Redress));
    app.update();
    app.update();
    assert_eq!(visibility(&app, flag), Some(Visibility::Inherited));
    assert_eq!(visibility(&app, hull), Some(Visibility::Hidden));
    assert!(app.world().get::<Redress>(after).is_none());
    // Its hierarchy: the root pivot plays its clips, the root's own mesh is its hull, the arm's
    // its flag; each pivot is a target by the names from the root to it.
    let [root, arm] = ["root", "arm"].map(|name| node(&app, name).unwrap());
    let pivots = app.world().get::<Dressed>(after).unwrap().pivots.clone();
    let meshes = |pivot: &str, meshes: Vec<Entity>| PivotMeshes {
        pivot: pivot.to_owned(),
        meshes,
    };
    assert_eq!(
        pivots,
        Some(ModelPivots {
            player: root,
            meshes: vec![meshes("root", vec![hull]), meshes("arm", vec![flag])],
        })
    );
    assert!(app.world().get::<AnimationPlayer>(root).is_some());
    let target = |path: &[&'static str]| {
        let names: Vec<Name> = path.iter().map(|&name| Name::new(name)).collect();
        AnimationTargetId::from_names(names.iter())
    };
    assert_eq!(
        app.world().get::<AnimationTargetId>(root),
        Some(&target(&["root"]))
    );
    assert_eq!(
        app.world().get::<AnimationTargetId>(arm),
        Some(&target(&["root", "arm"]))
    );
    assert_eq!(
        app.world().get::<AnimatedBy>(arm).map(|by| by.0),
        Some(root)
    );
    assert!(app.world().get::<AnimationTargetId>(hull).is_none());
}

#[test]
fn the_imports_materials_load_rough_of_no_reflectance_unlit_and_of_extras_where_they_say() {
    let (app, model) = loaded(MATERIALS);
    let server = app.world().resource::<AssetServer>().clone();
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
        rounds: Vec::new(),
        model: PackagePath::parse(path).unwrap(),
        scale: ModelScale::new(scale).unwrap(),
        hide: hide.iter().map(|&node| node.to_owned()).collect(),
        top: None,
        upgrades: Vec::new(),
        bone: None,
    };
    // An oak of two models, one at 1.5 and one hiding its stump; a bush of one.
    let objects = BTreeMap::from([
        (
            name("oak"),
            ObjectLook::new(vec![
                model("client/models/oak.glb", 1.5, &[]),
                model("client/models/leaves.glb", 1.0, &["leaves.stump"]),
            ]),
        ),
        (
            name("bush"),
            ObjectLook::new(vec![model("client/models/bush.glb", 1.0, &[])]),
        ),
    ]);
    let looks = ClientUnits::new(BTreeMap::new(), BTreeMap::new(), objects).unwrap();
    // The oak at (3, −2), 1 m up, turned 90°; the bush at the origin; and a ghost, whose look
    // the package lacks.
    let object = |look: &str, x: i64, z: i64, height: i64, angle: i64| ClientObject {
        look: name(look),
        position: [Num::int(x), Num::int(z)],
        height: Num::int(height),
        angle: Num::int(angle),
    };
    app.insert_resource(GroundHeights::of(grid, None));
    let data = ClientLoad {
        objects: ClientObjects {
            objects: vec![
                object("oak", 3, -2, 1, 90),
                object("bush", 0, 0, 0, 0),
                object("ghost", 1, 1, 0, 0),
            ],
        },
        ..ClientLoad::of_looks([looks])
    };
    app.insert_resource(data);
    app.init_resource::<ModelScenes>();
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
        turret_pitch: None,
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
    let [turret, elevation, flash, hull, other] = [(); 5].map(|()| world.spawn_empty().id());
    // It hides the flash and a decal, turns the turret and pitches its elevation.
    let look_nodes = LookNodes {
        hide: ["tank.decal".to_owned(), "tank.flash".to_owned()].into(),
        turret: Some("tank.turret".into()),
        turret_pitch: Some("tank.elevation".into()),
    };
    let node = |entity, name| Node { entity, name };
    let nodes = [
        node(turret, Some("tank.turret")),
        node(elevation, Some("tank.elevation")),
        node(flash, Some("tank.flash")),
        node(hull, Some("tank.hull")),
        node(other, None),
    ];
    assert_eq!(
        Dressing::of(&look_nodes, nodes.into_iter()),
        Dressing {
            hidden: vec![flash],
            turret: Some(turret),
            turret_pitch: Some(elevation),
        }
    );
    // A look of no turret turns none.
    let still = LookNodes {
        turret: None,
        turret_pitch: None,
        ..look_nodes
    };
    let dressing = Dressing::of(&still, nodes.into_iter());
    assert_eq!([dressing.turret, dressing.turret_pitch], [None, None]);
}

/// Turns each turret's nodes to `aim`, as the drawings' turn does each frame.
fn turned(
    In(aim): In<TurretAim>,
    mut drawings: Query<'_, '_, &mut TurretNodes>,
    mut nodes: Query<'_, '_, &mut Transform>,
) {
    for mut turret in &mut drawings {
        turret.turn(aim, &mut nodes);
    }
}

#[test]
fn a_turrets_nodes_turn_about_their_own_up_and_side_from_their_rest_or_their_clips_pose() {
    // A turret node at rest tilted 10° about x, turned 90°: a quarter turn about its own up
    // after the tilt, so its forward, +x, points along the tilted −z. Its elevation at rest
    // turned 90° about up, raised 30°: about its own side, its forward rises 30° along −z.
    let tilt = Quat::from_rotation_x(10_f32.to_radians());
    let quarter = Quat::from_rotation_y(FRAC_PI_2);
    let mut world = World::new();
    let [turret, elevation] =
        [tilt, quarter].map(|rotation| world.spawn(Transform::from_rotation(rotation)).id());
    world.spawn(TurretNodes {
        angle: Some(TurretNode {
            node: turret,
            base: tilt,
        }),
        pitch: Some(TurretNode {
            node: elevation,
            base: quarter,
        }),
    });
    let turn = world.register_system(turned);
    let at = |world: &mut World, angle: i64, pitch: i64| {
        let aim = TurretAim::of(Num::int(angle), Num::int(pitch));
        world.run_system_with(turn, aim).unwrap();
        [turret, elevation].map(|node| world.get::<Transform>(node).unwrap().rotation)
    };
    let [turned, raised] = at(&mut world, 90, 30);
    assert!((turned * Vec3::X).abs_diff_eq(tilt * Vec3::NEG_Z, 1e-6));
    let (sin, cos) = 30_f32.to_radians().sin_cos();
    assert!((raised * Vec3::X).abs_diff_eq(Vec3::new(0.0, sin, -cos), 1e-6));
    // At rest, each node keeps its rest, as no clip wrote it.
    assert_eq!(at(&mut world, 0, 0), [tilt, quarter]);
    // A clip that poses the turret's node 20° about up turns it from that pose: 20° and 90°.
    world.increment_change_tick();
    let posed = Quat::from_rotation_y(20_f32.to_radians());
    world.get_mut::<Transform>(turret).unwrap().rotation = posed;
    let [turned, _] = at(&mut world, 90, 0);
    let both = posed * quarter;
    assert!((turned * Vec3::X).abs_diff_eq(both * Vec3::X, 1e-6));
    // A frame its clip does not write keeps the pose, turned once, not again.
    assert_eq!(at(&mut world, 90, 0)[0], turned);
}

#[test]
fn an_animations_extension_gives_its_clip_the_timing_of_its_frames_and_hidden_pivots() {
    // A root and an arm on it, its one track the arm's of one key, and the extension: 4 frames
    // at 30 a second, the arm hidden at frames 1 and 2.
    let text = r#"{"asset":{"version":"2.0"},"extensionsUsed":["CAMPFIRE_animation_extras"],
"scene":0,"scenes":[{"nodes":[0]}],"nodes":[{"name":"root","children":[1]},{"name":"arm"}],
"animations":[{"channels":[{"sampler":0,"target":{"node":1,"path":"translation"}}],
"samplers":[{"input":0,"output":1}],
"extensions":{"CAMPFIRE_animation_extras":{"frames":4,"rate":30,"hidden":[{"node":1,"runs":[[1,3]]}]}}}],
"accessors":[{"bufferView":0,"componentType":5126,"count":1,"type":"SCALAR","min":[0.0],"max":[0.0]},
{"bufferView":1,"componentType":5126,"count":1,"type":"VEC3"}],
"bufferViews":[{"buffer":0,"byteLength":4},{"buffer":0,"byteOffset":4,"byteLength":12}],
"buffers":[{"byteLength":16,"uri":"data:application/octet-stream;base64,AAAAAAAAAAAAAAAAAAAAAA=="}]}"#;
    // The timing is asked for as the client asks for it, before its file loads, which holds it.
    let mut app = gltf_app(text);
    let server = app.world().resource::<AssetServer>().clone();
    let timing: Handle<ClipTiming> = server.load("model.gltf#Animation0/timing");
    wait(&mut app, &timing);
    let timing = app.world().resource::<Assets<ClipTiming>>().get(&timing);
    assert_eq!(
        timing,
        Some(&ClipTiming {
            frames: 4,
            rate: 30,
            hidden: vec![HiddenPivot {
                pivot: "arm".to_owned(),
                runs: vec![[1, 3]],
            }],
        })
    );
    // Its 4 frames last 2/15 s. A stride of 2 m at 15 m a second takes as long, so its rate
    // stays; a stride of 1 m takes half, so it plays twice as fast; at 7.5 m a second, as long.
    let timing = timing.unwrap();
    assert_eq!(
        [
            timing.walked(2.0, 15.0),
            timing.walked(1.0, 15.0),
            timing.walked(1.0, 7.5)
        ],
        [1.0, 2.0, 1.0]
    );
}

#[test]
fn a_model_at_a_bone_stands_where_its_hosts_node_stands_at_its_own_scale() {
    // A host of scale 2 whose `turret`, a meter up and turned 90° about up, holds `firepoint01`
    // a meter along its +x: the node stands at 2 × (0, 1, −1), as the turn takes +x to −z, turned
    // 90°, and a model of scale 0.5 at it stands there of its own scale, 0.5, as the host's scale
    // moves its nodes alone (riders-and-upgrades.md, A model may be drawn at another's bone). The
    // tolerance is the `f32` rounding of the quarter turn's sine and cosine.
    let mut world = World::new();
    let quarter = Quat::from_rotation_y(FRAC_PI_2);
    let host = world.spawn(Transform::from_scale(Vec3::splat(2.0))).id();
    let undressed = Dressed {
        hidden: Vec::new(),
        pivots: None,
    };
    let turret = Transform::from_xyz(0.0, 1.0, 0.0).with_rotation(quarter);
    let turret = world
        .spawn((turret, Name::new("turret"), ChildOf(host)))
        .id();
    let point = Transform::from_xyz(1.0, 0.0, 0.0);
    world.spawn((point, Name::new("firepoint01"), ChildOf(turret)));
    let attached = AtBone::new(host, Arc::from("firepoint01"), 0.5);
    let model = world.spawn((attached, Transform::IDENTITY)).id();
    let follow = world.register_system(UnitModelsPlugin::follow_bones);
    // Before its host is dressed, the model waits where it stands.
    world.run_system(follow).unwrap();
    assert_eq!(*world.get::<Transform>(model).unwrap(), Transform::IDENTITY);
    world.entity_mut(host).insert(undressed.clone());
    world.run_system(follow).unwrap();
    let stands = *world.get::<Transform>(model).unwrap();
    assert!(
        stands
            .translation
            .abs_diff_eq(Vec3::new(0.0, 2.0, -2.0), 1e-6)
    );
    assert!(stands.rotation.abs_diff_eq(quarter, 1e-6));
    assert_eq!(stands.scale, Vec3::splat(0.5));
    // A host of no such node leaves the model as it stands, the node missing for good.
    let bare = world.spawn((Transform::IDENTITY, undressed)).id();
    let lost = AtBone::new(bare, Arc::from("firepoint01"), 0.5);
    let lost = world.spawn((lost, Transform::from_xyz(5.0, 0.0, 0.0))).id();
    world.run_system(follow).unwrap();
    assert_eq!(
        *world.get::<Transform>(lost).unwrap(),
        Transform::from_xyz(5.0, 0.0, 0.0)
    );
    assert_eq!(world.get::<AtBone>(lost).unwrap().node, BoneNode::Missing);
    world.spawn((Name::new("firepoint01"), ChildOf(bare)));
    world.run_system(follow).unwrap();
    assert_eq!(world.get::<AtBone>(lost).unwrap().node, BoneNode::Missing);
}
