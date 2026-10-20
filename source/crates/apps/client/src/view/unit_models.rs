use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use bevy::app::{App, Plugin, Startup, Update};
use bevy::asset::{AssetServer, Handle, LoadContext, embedded_asset};
use bevy::camera::visibility::Visibility;
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::name::Name;
use bevy::ecs::observer::On;
use bevy::ecs::query::{Added, Allow, Changed, With};
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, Query, Res};
use bevy::ecs::world::EntityWorldMut;
use bevy::gltf::GltfMaterial;
use bevy::gltf::extensions::{
    ErasedGltfExtensionHandler, GltfExtensionHandler, GltfExtensionHandlers,
};
use bevy::gltf::gltf::{Material, Mesh, Primitive};
use bevy::math::Quat;
use bevy::mesh::Mesh3d;
use bevy::pbr::{MaterialPlugin, MeshMaterial3d, StandardMaterial};
use bevy::transform::components::Transform;
use bevy::world_serialization::WorldInstanceReady;
use campfire_client_files::{ClientObject, MaterialExtras};
use campfire_combat::TurretAngle;
use campfire_common::DeclaredName;
use campfire_sim::Unpredicted;
use campfire_units::TypeOriginBook;
use campfire_values::{PackageIndex, Team};
use serde::Deserialize;
use tracing::error;

use crate::view::client_data::ClientData;
use crate::view::drawing::{Drawing, DrawingOf};
use crate::view::file_material::{FileBlend, FileMaterial};
use crate::view::float_num::FloatNum;
use crate::view::ground_heights::GroundHeights;
use crate::view::match_colors::{MatchColors, Paint};
use crate::view::unit_looks::{LookModel, LookNodes, UnitLooks};

/// Draws units and the map's client objects with their packages' models: loads each unit type's
/// models, each primitive in the file material of its glTF material and its extras, once for each
/// model as it loads, places each client object still, and dresses each model once its scene
/// spawns: the nodes its look hides hidden, the node its turret angle turns found, as a look's
/// hidden nodes are its own, and each mesh tagged with its paint, which it paints again as its
/// paint changes (02-engine-core.md, Material extras in a glTF extension;
/// 02-engine-core.md, Team colors in the one material).
#[derive(Debug)]
pub(crate) struct UnitModelsPlugin;

/// The root of a client object's drawing, which stands still where the map places it, of no unit.
#[derive(Component, Debug)]
pub(crate) struct StillObject;

/// The glTF loader's part that gives each primitive the file material of its glTF material, as a
/// model loads.
#[derive(Debug, Default, Clone)]
struct ExtrasHandler;

impl Plugin for UnitModelsPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "file_material.wgsl");
        app.add_plugins(MaterialPlugin::<FileMaterial>::default());
        // The glTF loader takes its handlers as it is finished, after every plugin's build.
        app.world_mut()
            .resource_mut::<GltfExtensionHandlers>()
            .0
            .write_blocking()
            .push(Box::new(ExtrasHandler));
        app.add_systems(
            Startup,
            (MatchColors::upload, (Self::load_looks, Self::place_objects)).chain(),
        );
        app.add_observer(Self::dress);
        app.add_systems(
            Update,
            (
                Self::turn_turrets,
                (Self::follow_teams, Self::repaint).chain(),
            ),
        );
    }
}

impl UnitModelsPlugin {
    /// Loads every unit type's models, once the match's types are known.
    fn load_looks(
        origins: Res<'_, TypeOriginBook>,
        data: Res<'_, ClientData>,
        assets: Res<'_, AssetServer>,
        mut commands: Commands<'_, '_>,
    ) {
        commands.insert_resource(UnitLooks::of(&origins, &data, &assets));
    }

    /// Places each of the map's client objects (09-zero-hour.md, The multiplayer rule for client
    /// objects): a root on the ground at its point, at its height above the heightmap, turned by
    /// its angle as a placed unit is, and its look's models over it, each at its scale; a look the
    /// mode's package lacks is logged, and its object not drawn.
    fn place_objects(
        data: Res<'_, ClientData>,
        assets: Res<'_, AssetServer>,
        mut commands: Commands<'_, '_>,
    ) {
        let Some(looks) = data.packages.first().map(|package| &package.units) else {
            return;
        };
        let heights = data.heights.as_ref();
        // Each look's models resolve once, for every object of it.
        let mut resolved: BTreeMap<&DeclaredName, Vec<LookModel>> = BTreeMap::new();
        for object in &data.objects.objects {
            let models = match resolved.entry(&object.look) {
                Entry::Occupied(models) => models.into_mut(),
                Entry::Vacant(new) => {
                    let Some(look) = looks.objects.get(&object.look) else {
                        let look = object.look.as_str();
                        error!(look, "a client object's look is none of its package's");
                        continue;
                    };
                    new.insert(LookModel::all(PackageIndex::MODE, look, &assets))
                }
            };
            let root = commands
                .spawn((
                    StillObject,
                    Self::placed(object, heights),
                    Visibility::default(),
                ))
                .id();
            for model in &*models {
                commands.spawn((model.bundle(Paint::default()), ChildOf(root)));
            }
        }
    }

    /// Where `object` stands: its ground point, its height above the ground `heights` gives
    /// there, 0 on a map with none, and turned by its angle about the vertical.
    fn placed(object: &ClientObject, heights: Option<&GroundHeights>) -> Transform {
        let [x, z] = object.position.map(FloatNum::float);
        let beneath = heights.map_or(0.0, |heights| heights.at(x, z));
        let y = beneath + object.height.float();
        let turn = Quat::from_rotation_y(object.angle.float().to_radians());
        Transform::from_xyz(x, y, z).with_rotation(turn)
    }

    /// Turns the turret node of each drawing by its unit's turret angle, as the sim last gave it:
    /// of each unit whose angle changed, and of each drawing whose model was just dressed, so a
    /// node is written only when its angle or its node is new.
    fn turn_turrets(
        changed: TurnedTurrets<'_, '_>,
        dressed: Query<'_, '_, (&DrawingOf, &TurretNode), Added<TurretNode>>,
        (angles, turrets): (
            Query<'_, '_, &TurretAngle, Allow<Unpredicted>>,
            Query<'_, '_, &TurretNode>,
        ),
        mut nodes: Query<'_, '_, &mut Transform>,
    ) {
        let changed = changed.iter().filter_map(|(&angle, drawing)| {
            turrets
                .get(drawing.root())
                .ok()
                .map(|&turret| (angle, turret))
        });
        let dressed = dressed.iter().filter_map(|(&DrawingOf(unit), &turret)| {
            angles.get(unit).ok().map(|&angle| (angle, turret))
        });
        for (angle, turret) in changed.chain(dressed) {
            if let Ok(mut node) = nodes.get_mut(turret.node) {
                node.rotation = turret.rotation(angle);
            }
        }
    }

    /// Paints the models of each unit whose team changed, as a change of owner moves it to
    /// another team.
    fn follow_teams(
        units: Recolored<'_, '_>,
        roots: Query<'_, '_, &Children>,
        mut paints: Query<'_, '_, &mut Paint>,
    ) {
        for (&team, drawing) in &units {
            let Ok(models) = roots.get(drawing.root()) else {
                continue;
            };
            let mut models = paints.iter_many_mut(models);
            while let Some(paint) = models.fetch_next() {
                if let Ok(paint) = paint {
                    paint
                        .map_unchanged(|paint| &mut paint.team)
                        .set_if_neq(Some(team));
                }
            }
        }
    }

    /// Tags each mesh of each model whose paint changed, once its scene spawned; a model whose
    /// scene spawns after is tagged as it is dressed.
    fn repaint(
        models: Query<'_, '_, (Entity, &Paint), Changed<Paint>>,
        (children, meshes): (Query<'_, '_, &Children>, Query<'_, '_, (), With<Mesh3d>>),
        colors: Res<'_, MatchColors>,
        mut commands: Commands<'_, '_>,
    ) {
        for (model, &paint) in &models {
            let tag = colors.tag(paint);
            for node in children.iter_descendants(model) {
                if meshes.contains(node) {
                    commands.entity(node).insert(tag.clone());
                }
            }
        }
    }

    /// Dresses the model whose scene spawned, as its dressing says, and tags its meshes with its
    /// paint.
    fn dress(
        ready: On<'_, '_, WorldInstanceReady>,
        models: Query<'_, '_, (&LookNodes, &Paint)>,
        (children, names, meshes): (
            Query<'_, '_, &Children>,
            Query<'_, '_, &Name>,
            Query<'_, '_, (), With<Mesh3d>>,
        ),
        (parents, transforms): (Query<'_, '_, &ChildOf>, Query<'_, '_, &Transform>),
        colors: Res<'_, MatchColors>,
        mut commands: Commands<'_, '_>,
    ) {
        let Ok((look_nodes, &paint)) = models.get(ready.entity) else {
            return;
        };
        let tag = colors.tag(paint);
        for node in children.iter_descendants(ready.entity) {
            if meshes.contains(node) {
                commands.entity(node).insert(tag.clone());
            }
        }
        let spawned = children.iter_descendants(ready.entity).map(|node| Node {
            entity: node,
            name: names.get(node).ok().map(Name::as_str),
        });
        let Dressing { hidden, turret } = Dressing::of(look_nodes, spawned);
        for node in hidden {
            commands.entity(node).insert(Visibility::Hidden);
        }
        // A model is a child of its unit's drawing, which the turret's node turns with.
        if let Some(node) = turret
            && let (Ok(root), Ok(rest)) = (parents.get(ready.entity), transforms.get(node))
        {
            commands.entity(root.parent()).insert(TurretNode {
                node,
                rest: rest.rotation,
            });
        }
    }
}

impl ExtrasHandler {
    /// The extras of `material`, which its extension holds; none for a material of no extras, and
    /// none, logged, for extras that do not read.
    fn extras_of(material: &Material<'_>) -> Option<MaterialExtras> {
        let value = material.extension_value(MaterialExtras::EXTENSION)?;
        MaterialExtras::deserialize(value)
            .map_err(|problem| error!(error = %problem, "a material's extras do not read"))
            .ok()
    }

    /// The label of the file material of the glTF material of `label`.
    fn file_label(label: &str) -> String {
        format!("{label}/file")
    }
}

impl GltfExtensionHandler for ExtrasHandler {
    fn dyn_clone(&self) -> Box<dyn ErasedGltfExtensionHandler> {
        Box::new(Self)
    }

    /// Adds the file material of a glTF material, of the material as Bevy reads it and its
    /// extras, or the extras of its alpha mode for a material of none.
    fn on_material(
        &mut self,
        load_context: &mut LoadContext<'_>,
        gltf_material: &Material<'_>,
        _material: Handle<GltfMaterial>,
        material_asset: &GltfMaterial,
        material_label: &str,
    ) {
        let extras =
            Self::extras_of(gltf_material).unwrap_or_else(|| FileBlend::plain(material_asset));
        let file = FileBlend::material(material_asset, extras);
        load_context.add_labeled_asset(Self::file_label(material_label), file);
    }

    /// Draws a primitive by its file material, in place of the standard material Bevy's own part
    /// gives it.
    fn on_spawn_mesh_and_material(
        &mut self,
        load_context: &mut LoadContext<'_>,
        _primitive: &Primitive<'_>,
        _mesh: &Mesh<'_>,
        _material: &Material<'_>,
        entity: &mut EntityWorldMut<'_>,
        material_label: &str,
    ) {
        let file: Handle<FileMaterial> =
            load_context.get_label_handle(Self::file_label(material_label));
        entity
            .remove::<MeshMaterial3d<StandardMaterial>>()
            .insert(MeshMaterial3d(file));
    }
}

/// The units whose team changed, each with its drawing.
type Recolored<'w, 's> =
    Query<'w, 's, (&'static Team, &'static Drawing), (Changed<Team>, Allow<Unpredicted>)>;

/// The units whose turret angle changed, the client's own and those it only receives, each with
/// its drawing.
type TurnedTurrets<'w, 's> = Query<
    'w,
    's,
    (&'static TurretAngle, &'static Drawing),
    (Changed<TurretAngle>, Allow<Unpredicted>),
>;

/// A node of a spawned model: its entity, and its name.
#[derive(Debug, Clone, Copy)]
struct Node<'a> {
    entity: Entity,
    name: Option<&'a str>,
}

/// What a spawned model's dressing does: the nodes it hides, and the node its unit's turret angle
/// turns.
#[derive(Debug, Default, PartialEq, Eq)]
struct Dressing {
    hidden: Vec<Entity>,
    turret: Option<Entity>,
}

/// On a unit's drawing: the node of its model its turret angle turns, and that node's rotation
/// at rest, as its model holds it.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) struct TurretNode {
    node: Entity,
    rest: Quat,
}

impl TurretNode {
    /// The node's rotation at `angle`: its rest turned about the model's up by the angle, as the
    /// drawing turns by its unit's facing.
    fn rotation(&self, angle: TurretAngle) -> Quat {
        Quat::from_rotation_y(angle.degrees().float().to_radians()) * self.rest
    }
}

impl Dressing {
    /// The dressing of a model of `look_nodes`, whose scene spawned `nodes`: each node of a name it
    /// hides, and the first node of its turret's name.
    fn of<'a>(look_nodes: &LookNodes, nodes: impl Iterator<Item = Node<'a>>) -> Self {
        let mut dressing = Self::default();
        for node in nodes {
            if node
                .name
                .is_some_and(|name| look_nodes.hide.iter().any(|hidden| hidden == name))
            {
                dressing.hidden.push(node.entity);
            }
            let turret = look_nodes.turret.as_deref();
            if dressing.turret.is_none() && node.name.is_some_and(|name| Some(name) == turret) {
                dressing.turret = Some(node.entity);
            }
        }
        dressing
    }
}

#[cfg(test)]
mod tests {
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
    use bevy::world_serialization::{WorldAsset, WorldAssetRoot, WorldSerializationPlugin};
    use campfire_app_threads::OneThreadPlugins;
    use campfire_client_files::{
        ClientModel, ClientObjects, ClientUnit, ClientUnits, ModelScale, TeamColors,
    };
    use campfire_math::Num;
    use campfire_units::HeightGrid;
    use campfire_values::PackagePath;

    use super::*;
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
            let handle =
                server.get_handle::<FileMaterial>(format!("model.gltf#Material{at}/file"))?;
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
        let mut app = App::new();
        app.add_plugins((OneThreadPlugins, AssetPlugin::default()));
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
}
