use std::sync::Arc;

use bevy::app::{App, Plugin, Startup};
use bevy::asset::Assets;
use bevy::ecs::resource::Resource;
use bevy::ecs::world::{Mut, World};
use bevy::image::Image;
use bevy::material::AlphaMode;
use bevy::mesh::{Mesh, Mesh3d};
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use campfire_package::{ModePackages, PackageReader};

use crate::view::client_data::ClientData;
use crate::view::terrain_image::TerrainImage;
use crate::view::terrain_meshes::TerrainMeshes;

/// Draws a map's terrain, its file as the client data keeps it (design 16): composes its texture
/// of the mode's own package's textures as the client starts, and lays its ground and its
/// overlays over the map's heights, lit by the map's lights as every model is. A map with no
/// terrain draws none.
#[derive(Debug)]
pub(crate) struct TerrainView {
    pub(crate) packages: Arc<ModePackages>,
}

/// The mode's own package, whose textures a terrain's copies name.
#[derive(Resource, Debug)]
struct ModeFiles(PackageReader);

/// The overlays' lift over the ground, so they draw over the quads they share a depth with.
const OVERLAY_BIAS: f32 = 1.0;

impl Plugin for TerrainView {
    fn build(&self, app: &mut App) {
        let files = &self
            .packages
            .packages()
            .next()
            .expect("a mode has its own package")
            .package
            .files;
        app.insert_resource(ModeFiles(files.clone()));
        app.add_systems(Startup, TerrainView::draw);
    }
}

impl TerrainView {
    /// Composes the map's terrain's texture, builds its meshes, and spawns the ground and its
    /// overlays, each of the file's surface.
    fn draw(world: &mut World) {
        let ModeFiles(files) = world
            .remove_resource::<ModeFiles>()
            .expect("the plugin inserts the mode's files");
        world.resource_scope(|world, data: Mut<'_, ClientData>| {
            let (Some(file), Some(heights)) = (&data.terrain, &data.heights) else {
                return;
            };
            let grid = heights.grid();
            let image = TerrainImage::compose(file.texture(), &files);
            let TerrainMeshes { ground, overlay } =
                TerrainMeshes::of(file.quads(), file.texture().size, grid);
            let texture = world
                .resource_mut::<Assets<Image>>()
                .add(image.into_image());
            let surface = file.surface();
            // The surface's reflectance is half its specular factor, as Bevy's glTF loader reads
            // `KHR_materials_specular` and the client reads a material file's.
            let ground_material = StandardMaterial {
                base_color_texture: Some(texture),
                metallic: surface.metallic,
                perceptual_roughness: surface.roughness,
                reflectance: surface.specular * 0.5,
                ..StandardMaterial::default()
            };
            let overlay_material = StandardMaterial {
                alpha_mode: AlphaMode::Blend,
                depth_bias: OVERLAY_BIAS,
                ..ground_material.clone()
            };
            let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
            let ground_material = materials.add(ground_material);
            let overlay_material = materials.add(overlay_material);
            let mut meshes = world.resource_mut::<Assets<Mesh>>();
            let ground = meshes.add(ground.into_mesh());
            let overlay = (!overlay.indices.is_empty()).then(|| meshes.add(overlay.into_mesh()));
            world.spawn((Mesh3d(ground), MeshMaterial3d(ground_material)));
            if let Some(overlay) = overlay {
                world.spawn((Mesh3d(overlay), MeshMaterial3d(overlay_material)));
            }
        });
    }
}
