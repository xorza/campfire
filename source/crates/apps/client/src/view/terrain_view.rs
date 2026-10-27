use std::sync::Arc;

use bevy::app::{App, Plugin, Startup};
use bevy::asset::Assets;
use bevy::ecs::world::{Mut, World};
use bevy::image::Image;
use bevy::mesh::{Mesh, Mesh3d};
use bevy::pbr::{MeshMaterial3d, StandardMaterial};

use campfire_client_files::Minimap;

use crate::client_packages::ClientPackages;
use crate::view::client_data::ClientLoad;
use crate::view::file_material::{FileBlend, FileMaterial, FileMaterialPlugin};
use crate::view::ground_heights::GroundHeights;
use crate::view::match_hud::MatchHud;
use crate::view::minimap::{MinimapGround, TerrainSource};
use crate::view::terrain_image::TerrainImage;
use crate::view::terrain_meshes::TerrainMeshes;

/// Draws a map's terrain, its file as the client data keeps it (02-engine-core.md, Client):
/// composes its texture of the mode's own package's textures as the client starts, and lays its
/// ground, its overlays and its decals over the map's heights, lit by the map's lights as every
/// model is. A map with no terrain draws none.
#[derive(Debug)]
pub(crate) struct TerrainViewPlugin;

/// The overlays' lift over the ground, so they draw over the quads they share a depth with.
const OVERLAY_BIAS: i32 = 1;

impl Plugin for TerrainViewPlugin {
    fn build(&self, app: &mut App) {
        FileMaterialPlugin::require(app);
        app.add_systems(Startup, Self::draw);
    }
}

impl TerrainViewPlugin {
    /// Composes the map's terrain's texture, builds its meshes, and spawns the ground and its
    /// overlays, each of the file's surface.
    fn draw(world: &mut World) {
        let packages = Arc::clone(&world.resource::<ClientPackages>().packages);
        let files = &packages.mode().package.files;
        world.resource_scope(|world, load: Mut<'_, ClientLoad>| {
            let (Some(file), Some(heights)) =
                (&load.terrain, world.get_resource::<GroundHeights>())
            else {
                return;
            };
            let grid = heights.grid();
            let image = TerrainImage::compose(file.texture(), files);
            // The minimap's terrain, of the HUD's look, as the composed texture shows it
            // (02-engine-core.md, The minimap is drawn from the terrain at load).
            let look = world
                .get_resource::<MatchHud>()
                .map_or_else(Minimap::default, |hud| hud.0.minimap.clone());
            let terrain = TerrainSource {
                image: &image,
                quads: file.quads(),
                grid,
            };
            let minimap = MinimapGround::of(terrain, &look);
            let TerrainMeshes { ground, overlay } = TerrainMeshes::of(file, grid);
            world.insert_resource(minimap);
            let texture = world
                .resource_mut::<Assets<Image>>()
                .add(image.into_image());
            let surface = file.surface();
            // The surface's reflectance is half its specular factor, as Bevy's glTF loader reads
            // `KHR_materials_specular`.
            let ground_material = StandardMaterial {
                base_color_texture: Some(texture),
                metallic: surface.metallic,
                perceptual_roughness: surface.roughness,
                reflectance: surface.specular * 0.5,
                ..StandardMaterial::default()
            };
            let overlay_material = FileBlend::ground(ground_material.clone(), OVERLAY_BIAS);
            let mut materials = world.resource_mut::<Assets<FileMaterial>>();
            let overlay_material = materials.add(overlay_material);
            let ground_material = materials.add(FileBlend::opaque(ground_material));
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
