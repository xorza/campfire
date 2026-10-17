use std::collections::BTreeMap;

use bevy::app::{App, Plugin, Startup, Update};
use bevy::asset::{AssetServer, Assets, Handle, embedded_asset};
use bevy::camera::visibility::Visibility;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::name::Name;
use bevy::ecs::observer::On;
use bevy::ecs::query::{Added, Allow, Changed};
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Query, Res, ResMut};
use bevy::gltf::GltfMaterialName;
use bevy::image::{
    Image, ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor,
};
use bevy::math::{Quat, Vec3};
use bevy::pbr::{MaterialPlugin, MeshMaterial3d, StandardMaterial};
use bevy::transform::components::Transform;
use bevy::world_serialization::{WorldAssetRoot, WorldInstanceReady};
use campfire_client_files::{ClientObject, MaterialFile};
use campfire_combat::TurretAngle;
use campfire_sim::Unpredicted;
use campfire_units::TypeOriginBook;
use campfire_values::PackageIndex;

use tracing::error;

use crate::view::client_data::ClientData;
use crate::view::drawing::{Drawing, DrawingOf};
use crate::view::file_material::{FileBlend, FileMaterial};
use crate::view::float_num::FloatNum;
use crate::view::ground_heights::GroundHeights;
use crate::view::package_source::PackageSource;
use crate::view::unit_looks::{LookModel, ModelParts, UnitLooks};

/// Draws units and the map's client objects with their packages' models: loads each unit type's
/// models, places each client object still, and dresses each model once its scene spawns: the
/// nodes its default look hides hidden, and each primitive whose glTF material has a material
/// file in that file's material.
#[derive(Debug)]
pub(crate) struct UnitModels;

/// The root of a client object's drawing, which stands still where the map places it, of no unit.
#[derive(Component, Debug)]
pub(crate) struct StillObject;

/// On a model whose scene spawned and was dressed, so its nodes show as its look says.
#[derive(Component, Debug)]
pub(crate) struct Dressed;

/// The material of each material file once a primitive takes it, by its name, each by its
/// package's place.
#[derive(Resource, Debug, Default)]
struct FileMaterials {
    made: BTreeMap<PackageIndex, BTreeMap<String, Handle<FileMaterial>>>,
}

impl Plugin for UnitModels {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "file_material.wgsl");
        app.add_plugins(MaterialPlugin::<FileMaterial>::default());
        app.init_resource::<FileMaterials>();
        app.add_systems(Startup, (Self::load_looks, Self::place_objects));
        app.add_observer(Self::dress);
        app.add_systems(Update, Self::turn_turrets);
    }
}

impl UnitModels {
    /// Loads every unit type's models, once the match's types are known.
    fn load_looks(
        origins: Res<'_, TypeOriginBook>,
        data: Res<'_, ClientData>,
        assets: Res<'_, AssetServer>,
        mut commands: Commands<'_, '_>,
    ) {
        commands.insert_resource(UnitLooks::of(&origins, &data, &assets));
    }

    /// Places each of the map's client objects (12-zero-hour.md, The multiplayer rule for client
    /// objects): a root on the ground at its point, at its height above the heightmap, turned by
    /// its angle as a placed unit is, and its look's models over it, each at its scale; a look the
    /// mode's package lacks is logged, and its object not drawn.
    fn place_objects(
        data: Res<'_, ClientData>,
        assets: Res<'_, AssetServer>,
        mut commands: Commands<'_, '_>,
    ) {
        let Some(looks) = data.units.first() else {
            return;
        };
        let heights = data.heights.as_ref();
        for object in &data.objects.objects {
            let Some(look) = looks.objects.get(&object.look) else {
                let look = object.look.as_str();
                error!(look, "a client object's look is none of its package's");
                continue;
            };
            let root = commands
                .spawn((
                    StillObject,
                    Self::placed(object, heights),
                    Visibility::default(),
                ))
                .id();
            for model in LookModel::all(PackageIndex::MODE, look, &assets) {
                commands.spawn((
                    WorldAssetRoot(model.scene),
                    model.parts,
                    Transform::from_scale(Vec3::splat(model.scale)),
                    ChildOf(root),
                ));
            }
        }
    }

    /// Where `object` stands: its ground point, its height above the ground `heights` gives
    /// there, 0 on a map with none, and turned by its angle about the vertical.
    fn placed(object: &ClientObject, heights: Option<&GroundHeights>) -> Transform {
        let [x, z] = object.pos.map(FloatNum::float);
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

    /// Dresses the model whose scene spawned, as its dressing says, and marks it dressed.
    #[expect(
        clippy::too_many_arguments,
        reason = "a system takes each resource and query it reads"
    )]
    fn dress(
        ready: On<'_, '_, WorldInstanceReady>,
        models: Query<'_, '_, &ModelParts>,
        children: Query<'_, '_, &Children>,
        nodes: Query<'_, '_, (Option<&Name>, Option<&GltfMaterialName>)>,
        (parents, transforms): (Query<'_, '_, &ChildOf>, Query<'_, '_, &Transform>),
        data: Res<'_, ClientData>,
        assets: Res<'_, AssetServer>,
        mut made: ResMut<'_, FileMaterials>,
        mut materials: ResMut<'_, Assets<FileMaterial>>,
        mut commands: Commands<'_, '_>,
    ) {
        let Ok(parts) = models.get(ready.entity) else {
            return;
        };
        let spawned = children.iter_descendants(ready.entity).filter_map(|node| {
            let (name, material) = nodes.get(node).ok()?;
            Some(Node {
                entity: node,
                name: name.map(Name::as_str),
                material: material.map(|material| material.0.as_str()),
            })
        });
        let files = data.materials_of(parts.package);
        let Dressing {
            hidden,
            filed,
            turret,
        } = Dressing::of(parts, spawned, files);
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
        commands.entity(ready.entity).insert(Dressed);
        let made = made.made.entry(parts.package).or_default();
        for (node, name) in filed {
            let handle = if let Some(handle) = made.get(name) {
                handle.clone()
            } else {
                let file = &files[name];
                let handle = materials.add(Self::material(parts.package, file, &assets));
                made.insert(name.to_owned(), handle.clone());
                handle
            };
            commands
                .entity(node)
                .remove::<MeshMaterial3d<StandardMaterial>>()
                .insert(MeshMaterial3d(handle));
        }
    }

    /// The material of `file` of the package at `package`, its texture loaded as the glTF loads a
    /// base color: sRGB, wrapped as the file says.
    fn material(package: PackageIndex, file: &MaterialFile, assets: &AssetServer) -> FileMaterial {
        let texture = file.texture.as_ref().map(|texture| {
            let wrap = |clamp: bool| {
                if clamp {
                    ImageAddressMode::ClampToEdge
                } else {
                    ImageAddressMode::Repeat
                }
            };
            let (clamp_u, clamp_v) = (texture.clamp_u, texture.clamp_v);
            assets
                .load_builder()
                .with_settings(move |settings: &mut ImageLoaderSettings| {
                    settings.is_srgb = true;
                    settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                        address_mode_u: wrap(clamp_u),
                        address_mode_v: wrap(clamp_v),
                        ..ImageSamplerDescriptor::linear()
                    });
                })
                .load::<Image>(PackageSource::asset_path(package, &texture.path))
        });
        FileBlend::material(file, texture)
    }
}

/// The units whose turret angle changed, the client's own and those it only receives, each with
/// its drawing.
type TurnedTurrets<'w, 's> = Query<
    'w,
    's,
    (&'static TurretAngle, &'static Drawing),
    (Changed<TurretAngle>, Allow<Unpredicted>),
>;

/// A node of a spawned model: its entity, its name, and its glTF material's name, if it draws one.
#[derive(Debug, Clone, Copy)]
struct Node<'a> {
    entity: Entity,
    name: Option<&'a str>,
    material: Option<&'a str>,
}

/// What a spawned model's dressing does: the nodes it hides, the nodes whose material a material
/// file replaces, each with the file's name, and the node its unit's turret angle turns.
#[derive(Debug, Default, PartialEq, Eq)]
struct Dressing<'f> {
    hidden: Vec<Entity>,
    filed: Vec<(Entity, &'f str)>,
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

impl<'f> Dressing<'f> {
    /// The dressing of a model of `parts`, whose scene spawned `nodes`: each node of a name it
    /// hides, each node whose glTF material has a file among `files`, its model's package's, and
    /// the first node of its turret's name.
    fn of<'a>(
        parts: &ModelParts,
        nodes: impl Iterator<Item = Node<'a>>,
        files: &'f BTreeMap<String, MaterialFile>,
    ) -> Self {
        let mut dressing = Dressing::default();
        for node in nodes {
            if node
                .name
                .is_some_and(|name| parts.hide.iter().any(|hidden| hidden == name))
            {
                dressing.hidden.push(node.entity);
            }
            let turret = parts.turret.as_deref();
            if dressing.turret.is_none() && node.name.is_some_and(|name| Some(name) == turret) {
                dressing.turret = Some(node.entity);
            }
            if let Some((name, _)) = node
                .material
                .and_then(|material| files.get_key_value(material))
            {
                dressing.filed.push((node.entity, name.as_str()));
            }
        }
        dressing
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::FRAC_PI_2;
    use std::path::Path;

    use bevy::app::TaskPoolPlugin;
    use bevy::asset::io::memory::{Dir, MemoryAssetReader};
    use bevy::asset::io::{AssetSourceBuilder, AssetSourceId};
    use bevy::asset::{AssetApp, AssetPlugin, LoadState};
    use bevy::ecs::query::With;
    use bevy::ecs::world::World;
    use bevy::gltf::{Gltf, GltfMaterial, GltfPlugin};
    use bevy::mesh::MeshPlugin;
    use bevy::world_serialization::{WorldAsset, WorldSerializationPlugin};
    use campfire_client_files::{
        Blend, ClientModel, ClientObjects, ClientUnit, ClientUnits, ModelScale,
    };
    use campfire_common::DeclaredName;
    use campfire_math::Num;
    use campfire_units::HeightGrid;
    use campfire_values::PackagePath;

    use super::*;

    /// Two materials as the import writes them, each rough, not metallic and of a specular factor
    /// of 0, the second unlit.
    const MATERIALS: &str = r#"{"asset":{"version":"2.0","generator":"campfire-import"},
"extensionsUsed":["KHR_materials_specular","KHR_materials_unlit"],
"scene":0,"scenes":[{"nodes":[]}],"nodes":[],
"materials":[
{"name":"m_0","pbrMetallicRoughness":{"baseColorFactor":[1.0,0.5,0.0,1.0],"metallicFactor":0.0,
"roughnessFactor":1.0},"emissiveFactor":[0.0,0.0,0.0],"alphaMode":"OPAQUE","doubleSided":false,
"extensions":{"KHR_materials_specular":{"specularFactor":0.0}}},
{"name":"m_1","pbrMetallicRoughness":{"baseColorFactor":[1.0,1.0,1.0,1.0],"metallicFactor":0.0,
"roughnessFactor":1.0},"emissiveFactor":[0.0,0.0,0.0],"alphaMode":"BLEND","doubleSided":true,
"extensions":{"KHR_materials_specular":{"specularFactor":0.0},"KHR_materials_unlit":{}}}]}"#;

    #[test]
    fn the_imports_materials_load_in_bevy_rough_of_no_reflectance_and_unlit_where_they_say() {
        let dir = Dir::default();
        dir.insert_asset_text(Path::new("model.gltf"), MATERIALS);
        let mut app = App::new();
        let reader = MemoryAssetReader { root: dir };
        app.register_asset_source(
            AssetSourceId::Default,
            AssetSourceBuilder::new(move || Box::new(reader.clone())),
        );
        app.add_plugins((
            TaskPoolPlugin::default(),
            AssetPlugin::default(),
            WorldSerializationPlugin,
            MeshPlugin,
            GltfPlugin::default(),
        ));
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
    }

    #[test]
    fn each_client_object_stands_still_on_the_ground_turned_and_at_its_models_scales() {
        let mut app = App::new();
        app.add_plugins((TaskPoolPlugin::default(), AssetPlugin::default()));
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
            pos: [Num::int(x), Num::int(z)],
            height: Num::int(height),
            angle: Num::int(angle),
        };
        let data = ClientData {
            units: vec![looks],
            heights: Some(GroundHeights::of(grid, None)),
            objects: ClientObjects {
                objects: vec![
                    object("oak", 3, -2, 1, 90),
                    object("bush", 0, 0, 0, 0),
                    object("ghost", 1, 1, 0, 0),
                ],
            },
            ..ClientData::default()
        };
        app.insert_resource(data);
        app.add_systems(Startup, UnitModels::place_objects);
        app.update();
        let world = app.world_mut();
        let mut roots = world.query_filtered::<(&Transform, &Children), With<StillObject>>();
        let mut placed: Vec<(Transform, Vec<(Transform, ModelParts)>)> = roots
            .iter(world)
            .map(|(transform, children)| {
                let models = children
                    .iter()
                    .map(|child| {
                        let model = world.entity(*child);
                        assert!(model.get::<WorldAssetRoot>().is_some());
                        (
                            *model.get::<Transform>().unwrap(),
                            model.get::<ModelParts>().unwrap().clone(),
                        )
                    })
                    .collect();
                (*transform, models)
            })
            .collect();
        placed.sort_by(|a, b| a.0.translation.x.total_cmp(&b.0.translation.x));
        // The bush at the ground's 4 m, unturned; the oak at 4 + 1 = 5 m, turned a quarter about
        // y; each model at its own scale, with the nodes its look hides; the ghost is none.
        let parts = |hide: &[&str]| ModelParts {
            package: PackageIndex::MODE,
            hide: hide.iter().map(|&node| node.to_owned()).collect(),
            turret: None,
        };
        let scaled = |scale: f32| Transform::from_scale(Vec3::splat(scale));
        assert_eq!(placed.len(), 2);
        assert_eq!(placed[0].0, Transform::from_xyz(0.0, 4.0, 0.0));
        assert_eq!(placed[0].1, [(scaled(1.0), parts(&[]))]);
        let oak = placed[1].0;
        assert_eq!(oak.translation, Vec3::new(3.0, 5.0, -2.0));
        assert!(
            oak.rotation
                .abs_diff_eq(Quat::from_rotation_y(90.0_f32.to_radians()), 1e-6)
        );
        assert_eq!(
            placed[1].1,
            [
                (scaled(1.5), parts(&[])),
                (scaled(1.0), parts(&["leaves.stump"]))
            ]
        );
    }

    #[test]
    fn a_dressing_hides_the_named_nodes_and_gives_filed_materials_their_files() {
        let mut world = World::new();
        let [turret, flash, hull, other] = [(); 4].map(|()| world.spawn_empty().id());
        let file = MaterialFile {
            base_color: [1.0; 4],
            texture: None,
            emissive: [0.0; 3],
            metallic: 0.0,
            roughness: 1.0,
            specular: 0.0,
            unlit: false,
            blend: Blend::Add,
            depth_write: false,
            depth_bias: 0,
            double_sided: false,
        };
        // The files of the model's package: `tank_3`'s, and not `tank_0`'s.
        let files = BTreeMap::from([("tank_3".to_owned(), file)]);
        // It hides the flash and a decal, and turns the turret.
        let parts = ModelParts {
            package: PackageIndex::MODE,
            hide: ["tank.decal".to_owned(), "tank.flash".to_owned()].into(),
            turret: Some("tank.turret".into()),
        };
        let nodes = [
            Node {
                entity: turret,
                name: Some("tank.turret"),
                material: None,
            },
            Node {
                entity: flash,
                name: Some("tank.flash"),
                material: Some("tank_3"),
            },
            Node {
                entity: hull,
                name: Some("tank.hull"),
                material: Some("tank_0"),
            },
            Node {
                entity: other,
                name: None,
                material: Some("tank_3"),
            },
        ];
        assert_eq!(
            Dressing::of(&parts, nodes.into_iter(), &files),
            Dressing {
                hidden: vec![flash],
                filed: vec![(flash, "tank_3"), (other, "tank_3")],
                turret: Some(turret),
            }
        );
        // A look of no turret turns none.
        let still = ModelParts {
            turret: None,
            ..parts
        };
        assert_eq!(Dressing::of(&still, nodes.into_iter(), &files).turret, None);
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
