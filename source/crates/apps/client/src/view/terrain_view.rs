use std::sync::Arc;

use bevy::app::{App, Plugin, Startup};
use bevy::asset::Assets;
use bevy::ecs::resource::Resource;
use bevy::ecs::world::World;
use bevy::image::Image;
use bevy::material::AlphaMode;
use bevy::mesh::{Mesh, Mesh3d};
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use campfire_codec::Binary;
use campfire_package::{ModePackages, TerrainFile};
use campfire_values::PackagePath;
use tracing::error;

use crate::view::GroundDrawn;
use crate::view::client_data::ClientData;
use crate::view::terrain_image::TerrainImage;
use crate::view::terrain_meshes::TerrainMeshes;

/// Draws a map's terrain from its terrain file, `client/maps/<map>/terrain.bin` of the mode's own
/// package (design 16): composes its texture and reads it as the client starts, then lays its
/// ground and its overlays over the map's heights, lit by the map's lights as every model is. A
/// map with no terrain file draws none; a file that does not read is logged, and none is drawn.
#[derive(Debug)]
pub(crate) struct TerrainView {
    pub(crate) packages: Arc<ModePackages>,
}

/// The terrain file and its composed texture, until the terrain is drawn.
#[derive(Resource, Debug)]
struct TerrainLoad {
    file: TerrainFile,
    image: TerrainImage,
}

/// The overlays' lift over the ground, so they draw over the quads they share a depth with.
const OVERLAY_BIAS: f32 = 1.0;

impl Plugin for TerrainView {
    fn build(&self, app: &mut App) {
        let Some(load) = TerrainLoad::read(&self.packages) else {
            return;
        };
        app.insert_resource(load);
        app.insert_resource(GroundDrawn);
        app.add_systems(Startup, TerrainView::draw);
    }
}

impl TerrainView {
    /// Builds the meshes, and spawns the ground and its overlays, each of the file's surface; the
    /// file and its texture go with it.
    fn draw(world: &mut World) {
        let TerrainLoad { file, image } = world
            .remove_resource::<TerrainLoad>()
            .expect("a terrain is drawn once, of the load its plugin inserts");
        let Some(grid) = &world.resource::<ClientData>().grid else {
            error!("a map's terrain has no heights to lie on");
            return;
        };
        let Some(TerrainMeshes { ground, overlay }) =
            TerrainMeshes::of(file.quads(), file.texture().size, grid)
        else {
            error!("a map's terrain is not the shape of its heights");
            return;
        };
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
    }
}

impl TerrainLoad {
    /// The terrain file of `packages`' map in the mode's own package, and its texture composed of
    /// that package's textures; none for a map with no terrain file, and none, logged, for one
    /// that does not read.
    fn read(packages: &ModePackages) -> Option<TerrainLoad> {
        let files = &packages
            .packages()
            .next()
            .expect("a mode has its own package")
            .package
            .files;
        let path = TerrainFile::path(packages.map_name().as_str());
        let path = PackagePath::parse(&path).expect("a map's name is a package path's");
        let bytes = ClientData::read_file(files, &path)?;
        let file = ClientData::logged(&path, Binary::decode::<TerrainFile>(&bytes))?;
        let image = TerrainImage::compose(file.texture(), files);
        Some(TerrainLoad { file, image })
    }
}
