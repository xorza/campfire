use std::collections::BTreeMap;
use std::sync::Arc;

use bevy::app::{App, Plugin, Startup};
use bevy::asset::{AssetServer, Assets, Handle, embedded_asset};
use bevy::camera::visibility::Visibility;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::name::Name;
use bevy::ecs::observer::On;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Query, Res, ResMut};
use bevy::gltf::GltfMaterialName;
use bevy::image::{
    Image, ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor,
};
use bevy::pbr::{MaterialPlugin, MeshMaterial3d, StandardMaterial};
use bevy::world_serialization::WorldInstanceReady;
use campfire_capabilities::TypeOrigins;
use campfire_package::{MaterialFile, ModePackages};

use crate::view::client_data::ClientData;
use crate::view::file_material::{FileBlend, FileMaterial};
use crate::view::package_source::PackageSource;
use crate::view::unit_looks::{ModelParts, UnitLooks};

/// Draws units with their packages' models: reads the mode's client data as it starts, loads each
/// unit type's models, and dresses each model once its scene spawns: the nodes its default look
/// hides hidden, and each primitive whose glTF material has a material file in that file's
/// material.
#[derive(Debug)]
pub(crate) struct UnitModels {
    pub(crate) packages: Arc<ModePackages>,
}

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
        app.add_systems(Startup, UnitModels::load_looks);
        app.add_observer(UnitModels::dress);
    }
}

impl UnitModels {
    /// Loads every unit type's models, once the match's types are known.
    fn load_looks(
        origins: Res<'_, TypeOrigins>,
        data: Res<'_, ClientData>,
        assets: Res<'_, AssetServer>,
        mut commands: Commands<'_, '_>,
    ) {
        commands.insert_resource(UnitLooks::of(&origins, &data, &assets));
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
    use bevy::ecs::world::World;
    use campfire_package::Blend;

    use super::*;

    #[test]
    fn a_dressing_hides_the_named_nodes_and_gives_filed_materials_their_files() {
        let mut world = World::new();
        let [turret, flash, hull, other] = [(); 4].map(|()| world.spawn_empty().id());
        let file = MaterialFile {
            base_color: [1.0; 4],
            texture: None,
            emissive: [0.0; 3],
            blend: Blend::Add,
            depth_write: false,
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
