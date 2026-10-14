use std::collections::BTreeMap;
use std::sync::Arc;

use bevy::app::{App, Plugin, Startup};
use bevy::asset::{AssetServer, Assets, Handle, embedded_asset};
use bevy::camera::visibility::Visibility;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::name::Name;
use bevy::ecs::observer::On;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Query, Res, ResMut};
use bevy::gltf::GltfMaterialName;
use bevy::image::{
    Image, ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor,
};
use bevy::math::{Quat, Vec3};
use bevy::pbr::{MaterialPlugin, MeshMaterial3d, StandardMaterial};
use bevy::transform::components::Transform;
use bevy::world_serialization::WorldAssetRoot;
use bevy::world_serialization::WorldInstanceReady;
use campfire_package::{ClientObject, MaterialFile, ModePackages};
use campfire_units::TypeOriginBook;

use tracing::error;

use crate::view::client_data::ClientData;
use crate::view::file_material::{FileBlend, FileMaterial};
use crate::view::float_num::FloatNum;
use crate::view::ground_heights::GroundHeights;
use crate::view::package_source::PackageSource;
use crate::view::unit_looks::{LookModel, ModelParts, UnitLooks};

/// Draws units and the map's client objects with their packages' models: reads the mode's client
/// data as it starts, loads each unit type's models, places each client object still, and dresses
/// each model once its scene spawns: the nodes its default look hides hidden, and each primitive
/// whose glTF material has a material file in that file's material.
#[derive(Debug)]
pub(crate) struct UnitModels {
    pub(crate) packages: Arc<ModePackages>,
}

/// The root of a client object's drawing, which stands still where the map places it, of no unit.
#[derive(Component, Debug)]
pub(crate) struct StillObject;

/// The material of each material file once a primitive takes it, by its package's place and name.
#[derive(Resource, Debug, Default)]
struct FileMaterials {
    made: BTreeMap<(u16, String), Handle<FileMaterial>>,
}

impl Plugin for UnitModels {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "file_material.wgsl");
        app.add_plugins(MaterialPlugin::<FileMaterial>::default());
        app.insert_resource(ClientData::read(&self.packages));
        app.init_resource::<FileMaterials>();
        app.add_systems(Startup, (UnitModels::load_looks, UnitModels::place_objects));
        app.add_observer(UnitModels::dress);
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

    /// Places each of the map's client objects (design 12, D14): a root on the ground at its point,
    /// at its height above the heightmap, turned by its angle as a placed unit is, and its look's
    /// models over it, each at its scale; a look the mode's package lacks is logged, and its
    /// object not drawn.
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
                    UnitModels::placed(object, heights),
                    Visibility::default(),
                ))
                .id();
            for model in LookModel::all(0, look, &assets) {
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

    /// Dresses the model whose scene spawned, as its dressing says.
    #[expect(
        clippy::too_many_arguments,
        reason = "a system takes each resource and query it reads"
    )]
    fn dress(
        ready: On<'_, '_, WorldInstanceReady>,
        models: Query<'_, '_, &ModelParts>,
        children: Query<'_, '_, &Children>,
        nodes: Query<'_, '_, (Option<&Name>, Option<&GltfMaterialName>)>,
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
        let Dressing { hidden, files } = Dressing::of(parts, spawned, &data.materials);
        for node in hidden {
            commands.entity(node).insert(Visibility::Hidden);
        }
        for (node, key) in files {
            let file = &data.materials[&key];
            let handle = made
                .made
                .entry(key)
                .or_insert_with(|| {
                    materials.add(UnitModels::material(parts.package, file, &assets))
                })
                .clone();
            commands
                .entity(node)
                .remove::<MeshMaterial3d<StandardMaterial>>()
                .insert(MeshMaterial3d(handle));
        }
    }

    /// The material of `file` of the package at `package`, its texture loaded as the glTF loads a
    /// base color: sRGB, wrapped as the file says.
    fn material(package: u16, file: &MaterialFile, assets: &AssetServer) -> FileMaterial {
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

/// A node of a spawned model: its entity, its name, and its glTF material's name, if it draws one.
#[derive(Debug, Clone, Copy)]
struct Node<'a> {
    entity: Entity,
    name: Option<&'a str>,
    material: Option<&'a str>,
}

/// What a spawned model's dressing does: the nodes it hides, and the nodes whose material a
/// material file replaces, each with the file's key.
#[derive(Debug, Default, PartialEq, Eq)]
struct Dressing {
    hidden: Vec<Entity>,
    files: Vec<(Entity, (u16, String))>,
}

impl Dressing {
    /// The dressing of a model of `parts` whose scene spawned `nodes`: each node of a name its
    /// look hides, and each node whose glTF material has a file of its package among `files`.
    fn of<'a>(
        parts: &ModelParts,
        nodes: impl Iterator<Item = Node<'a>>,
        files: &BTreeMap<(u16, String), MaterialFile>,
    ) -> Dressing {
        let mut dressing = Dressing::default();
        for node in nodes {
            if node
                .name
                .is_some_and(|name| parts.hide.iter().any(|hidden| hidden == name))
            {
                dressing.hidden.push(node.entity);
            }
            if let Some(material) = node.material {
                let key = (parts.package, material.to_owned());
                if files.contains_key(&key) {
                    dressing.files.push((node.entity, key));
                }
            }
        }
        dressing
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use bevy::app::TaskPoolPlugin;
    use bevy::asset::io::memory::{Dir, MemoryAssetReader};
    use bevy::asset::io::{AssetSourceBuilder, AssetSourceId};
    use bevy::asset::{AssetApp, AssetPlugin, LoadState};
    use bevy::ecs::query::With;
    use bevy::ecs::world::World;
    use bevy::gltf::{Gltf, GltfMaterial, GltfPlugin};
    use bevy::mesh::MeshPlugin;
    use bevy::world_serialization::WorldAsset;
    use bevy::world_serialization::WorldSerializationPlugin;
    use campfire_math::Num;
    use campfire_mode::HeightGrid;
    use campfire_package::{
        Blend, ClientModel, ClientObjects, ClientUnit, ClientUnits, ModelScale,
    };
    use campfire_values::{DeclaredName, PackagePath};

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
                    ClientUnit {
                        models: vec![
                            model("client/models/oak.glb", 1.5, &[]),
                            model("client/models/leaves.glb", 1.0, &["leaves.stump"]),
                        ],
                    },
                ),
                (
                    name("bush"),
                    ClientUnit {
                        models: vec![model("client/models/bush.glb", 1.0, &[])],
                    },
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
            heights: GroundHeights::of(&grid),
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
            package: 0,
            hide: hide.iter().map(|&node| node.to_owned()).collect(),
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
        // Files for the tank's `tank_3` in package 1, and for `tank_0` in package 0 alone.
        let files = BTreeMap::from([
            ((1, "tank_3".to_owned()), file.clone()),
            ((0, "tank_0".to_owned()), file),
        ]);
        let parts = ModelParts {
            package: 1,
            hide: vec!["tank.turret".to_owned(), "tank.flash".to_owned()],
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
                hidden: vec![turret, flash],
                files: vec![
                    (flash, (1, "tank_3".to_owned())),
                    (other, (1, "tank_3".to_owned()))
                ],
            }
        );
    }
}
