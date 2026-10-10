use std::error::Error;
use std::sync::Arc;

use bevy::app::{App, Plugin, Startup};
use bevy::asset::{Assets, RenderAssetUsages};
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Res, ResMut};
use bevy::image::{Image, ImageSampler, ImageSamplerDescriptor};
use bevy::material::AlphaMode;
use bevy::mesh::{Mesh, Mesh3d};
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use bevy::render::render_resource::{Extent3d, TextureDataOrder, TextureDimension, TextureFormat};
use campfire_capabilities::PackagePath;
use campfire_common::{Binary, Toml};
use campfire_log::ErrorReport;
use campfire_package::{
    ClassTexture, ContentError, GameData, ModePackages, PackageReader, Terrain, TerrainAtlas,
};
use tracing::error;

use crate::view::GroundDrawn;
use crate::view::client_data::ClientData;
use crate::zero_hour::terrain_cells::TerrainCells;
use crate::zero_hour::terrain_mesh::TerrainMeshes;

/// Draws the terrain of a map imported from Zero Hour: reads its `client/maps/<map>/terrain.bin`,
/// the package's `client/game_data.toml` and each texture class's texture as the client starts,
/// and draws the ground of the map's heightmap with them, its tiles and its blends, in its own
/// light. A map with no
/// terrain file draws none; a file that does not read is logged, and none is drawn.
#[derive(Debug)]
pub(crate) struct TerrainView {
    pub(crate) packages: Arc<ModePackages>,
}

/// The files the terrain draws from, until it is drawn.
#[derive(Resource, Debug)]
struct TerrainFiles {
    terrain: Terrain,
    game: GameData,
    /// Each class's texture, by the class's index; none for one with no texture, or one that
    /// did not read.
    textures: Vec<Option<ClassTexture>>,
}

/// The overlay's lift over the ground, so its blends draw over the tiles they share a depth
/// with.
const OVERLAY_BIAS: f32 = 1.0;

impl Plugin for TerrainView {
    fn build(&self, app: &mut App) {
        let Some(files) = TerrainFiles::read(&self.packages) else {
            return;
        };
        app.insert_resource(files);
        app.insert_resource(GroundDrawn);
        app.add_systems(Startup, TerrainView::draw);
    }
}

impl TerrainView {
    /// Builds the atlas and the meshes, and spawns the ground and its overlay.
    fn draw(
        files: Res<'_, TerrainFiles>,
        data: Res<'_, ClientData>,
        mut images: ResMut<'_, Assets<Image>>,
        mut meshes: ResMut<'_, Assets<Mesh>>,
        mut materials: ResMut<'_, Assets<StandardMaterial>>,
        mut commands: Commands<'_, '_>,
    ) {
        commands.remove_resource::<TerrainFiles>();
        let Some(grid) = &data.grid else {
            error!("a map's terrain has no heights to lie on");
            return;
        };
        let parts = files.terrain.parts();
        if parts.cells.len() != grid.samples().len() || parts.columns != grid.columns() {
            error!("a map's terrain is not the shape of its heights");
            return;
        }
        let atlas = TerrainAtlas::new(parts, &files.textures);
        let cells = TerrainCells::new(parts, grid.samples(), &atlas, files.game);
        let TerrainMeshes { ground, overlay } =
            TerrainMeshes::of(&cells, grid, &atlas, files.terrain.lighting());
        let texture = images.add(TerrainView::atlas_image(atlas));
        // Each vertex holds its light, as the game lights the terrain itself.
        let ground_material = StandardMaterial {
            base_color_texture: Some(texture),
            unlit: true,
            ..StandardMaterial::default()
        };
        let overlay_material = StandardMaterial {
            alpha_mode: AlphaMode::Blend,
            depth_bias: OVERLAY_BIAS,
            ..ground_material.clone()
        };
        commands.spawn((
            Mesh3d(meshes.add(ground.into_mesh())),
            MeshMaterial3d(materials.add(ground_material)),
        ));
        if !overlay.indices.is_empty() {
            commands.spawn((
                Mesh3d(meshes.add(overlay.into_mesh())),
                MeshMaterial3d(materials.add(overlay_material)),
            ));
        }
    }

    /// The atlas as a texture of its three levels, sampled linearly between texels and levels.
    fn atlas_image(atlas: TerrainAtlas) -> Image {
        let size = Extent3d {
            width: TerrainAtlas::WIDTH,
            height: atlas.height(),
            depth_or_array_layers: 1,
        };
        let mut image = Image::new_uninit(
            size,
            TextureDimension::D2,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        );
        image.texture_descriptor.mip_level_count = 3;
        image.data_order = TextureDataOrder::MipMajor;
        image.data = Some(atlas.into_texels());
        image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::linear());
        image
    }
}

impl TerrainFiles {
    /// The terrain files of `packages`' map, in the mode's own package; none for a map with no
    /// terrain, or files that do not read, logged.
    fn read(packages: &ModePackages) -> Option<TerrainFiles> {
        let files = &packages
            .packages()
            .next()
            .expect("a mode has its own package")
            .package
            .files;
        let path = format!("client/maps/{}/terrain.bin", packages.map_name());
        let path = PackagePath::parse(&path).expect("a map's name is a package path's");
        let terrain = match files.read_file(&path) {
            Err(ContentError::Missing { .. }) => return None,
            read => logged(&path, read)?,
        };
        let terrain = logged(&path, Binary::decode::<Terrain>(&terrain))?;
        let game = PackagePath::parse(GameData::PATH).expect("a constant package path");
        let text = logged(&game, files.read_file(&game))?;
        let text = logged(&game, String::from_utf8(text))?;
        let game = logged(&game, Toml::parse::<GameData>(&text))?;
        let textures = terrain
            .parts()
            .classes
            .iter()
            .map(|class| TerrainFiles::texture(files, class.texture.as_ref()?))
            .collect();
        Some(TerrainFiles {
            terrain,
            game,
            textures,
        })
    }

    /// The texture at `path`; none, logged, for one that does not read, or is none the atlas
    /// takes.
    fn texture(files: &PackageReader, path: &PackagePath) -> Option<ClassTexture> {
        let bytes = logged(path, files.read_file(path))?;
        logged(path, ClassTexture::of_ktx2(&bytes))
    }
}

/// `result`'s value; none, logged with `path`, for its error.
fn logged<T, E: Error + 'static>(path: &PackagePath, result: Result<T, E>) -> Option<T> {
    result
        .map_err(|problem| error!(path = %path, error = %ErrorReport::of(&problem), "a terrain file does not read"))
        .ok()
}
