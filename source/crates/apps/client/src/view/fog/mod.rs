use bevy::app::{App, Plugin, PostStartup, Update};
use bevy::asset::{Assets, Handle, RenderAssetUsages, uuid_handle};
use bevy::ecs::query::{Allow, Changed, Or};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::schedule::common_conditions::resource_exists;
use bevy::ecs::system::{Commands, Local, Query, Res, ResMut};
use bevy::image::{Image, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Vec3;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::render::storage::ShaderBuffer;
use campfire_client_files::{FogFile, FogLevel};
use campfire_players::Player;
use campfire_sim::Unpredicted;
use campfire_vision::{CellBits, ExploredCells, SeenCells, VisionBook, VisionGrid};

use crate::own_player::OwnPlayer;
use crate::view::client_data::ClientLoad;
use crate::view::float_num::FloatNum;

/// The fog every file material darkens by: a texture of a texel for each cell of the map's vision
/// grid, the light its player's team keeps there, sampled between cells, and the frame that places
/// it on the ground (02-engine-core.md, The fog is the one material's). Before the match's grid,
/// and in a mode of no vision or for a spectator, every point keeps all of its light.
#[derive(Resource, Debug)]
pub(crate) struct MatchFog {
    file: FogFile,
    columns: usize,
    rows: usize,
    frame: [f32; 4],
    shroud: bool,
}

/// The players whose cells changed since the fog's last paint, whose entities the client holds
/// as the server sent them.
type ChangedCells<'w, 's> = Query<
    'w,
    's,
    (&'static Player, &'static SeenCells, &'static ExploredCells),
    (
        Or<(Changed<SeenCells>, Changed<ExploredCells>)>,
        Allow<Unpredicted>,
    ),
>;

/// The fog of a match whose mode has vision.
#[derive(Debug)]
pub(crate) struct FogPlugin;

impl Plugin for FogPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostStartup, MatchFog::load.before(ClientLoad::drop));
        app.add_systems(Update, MatchFog::paint.run_if(resource_exists::<MatchFog>));
    }
}

impl MatchFog {
    /// The texture of the fog, which every file material samples.
    pub(crate) const IMAGE: Handle<Image> = uuid_handle!("0b7f6f0e-3c55-4c8e-9a31-5d2e7f4a9c10");
    /// The frame that places the texture on the ground, `[x, z, 1 / width, 1 / depth]` of its
    /// first cell's corner and its whole size, in meters.
    pub(crate) const FRAME: Handle<ShaderBuffer> =
        uuid_handle!("7c1e4b2a-9f60-4d3b-8e25-1a6c3f9b0d72");

    /// Puts a fog of one clear texel, over all the ground, as the client starts: what every file
    /// material draws by until a match's grid gives its own.
    pub(crate) fn clear(
        mut images: ResMut<'_, Assets<Image>>,
        mut buffers: ResMut<'_, Assets<ShaderBuffer>>,
    ) {
        let clear = Self::image(1, 1, [u8::MAX; 4]);
        images
            .insert(Self::IMAGE.id(), clear)
            .expect("the fog's handle is its own");
        buffers
            .insert(Self::FRAME.id(), ShaderBuffer::from(vec![[0.0_f32; 4]]))
            .expect("the frame's handle is its own");
    }

    /// Puts the fog of the match's grid, each cell shrouded, or explored of a map that starts
    /// known, until its player's cells come, and the frame that places it.
    fn load(
        (load, vision): (Res<'_, ClientLoad>, Option<Res<'_, VisionBook>>),
        mut images: ResMut<'_, Assets<Image>>,
        mut buffers: ResMut<'_, Assets<ShaderBuffer>>,
        mut commands: Commands<'_, '_>,
    ) {
        let Some(grid) = vision.as_deref().and_then(VisionBook::grid) else {
            return;
        };
        let rows = grid.grid.rows();
        let fog = Self {
            file: load.fog,
            columns: grid.grid.columns(),
            rows,
            frame: Self::frame(grid),
            shroud: grid.shroud,
        };
        let mut image = Self::image(fog.columns, rows, [u8::MAX; 4]);
        let texels = image.data.as_mut().expect("a fog holds its texels");
        let none = CellBits::default();
        fog.texels(&none, &none, texels);
        images
            .insert(Self::IMAGE.id(), image)
            .expect("the fog's handle is its own");
        buffers
            .insert(Self::FRAME.id(), ShaderBuffer::from(vec![fog.frame]))
            .expect("the frame's handle is its own");
        commands.insert_resource(fog);
    }

    /// Writes the fog's texels again as its player's cells change, in place; a spectator's once,
    /// all of them seen.
    fn paint(
        fog: Res<'_, Self>,
        own: OwnPlayer<'_, '_>,
        players: ChangedCells<'_, '_>,
        mut images: ResMut<'_, Assets<Image>>,
        mut cleared: Local<'_, bool>,
    ) {
        let Some(own) = own.player_slot() else {
            // A spectator sees every point in all of its light, written once.
            if !*cleared {
                let mut image = images.get_mut(&Self::IMAGE).expect("the fog loaded");
                let texels = image.data.as_mut().expect("a fog holds its texels");
                fog.clear_texels(texels);
                *cleared = true;
            }
            return;
        };
        *cleared = false;
        let changed = players
            .iter()
            .find(|(player, _, _)| player.player_slot() == own);
        let Some((_, seen, explored)) = changed else {
            return;
        };
        let mut image = images.get_mut(&Self::IMAGE).expect("the fog loaded");
        let texels = image.data.as_mut().expect("a fog holds its texels");
        fog.texels(seen.cells(), explored.cells(), texels);
    }

    /// The cell of the grid under `at`, a point of the drawn world; none past the grid.
    #[expect(
        clippy::cast_sign_loss,
        reason = "a place is cast once it stands within 0 and its count"
    )]
    pub(crate) fn cell_at(&self, at: Vec3) -> Option<usize> {
        let [x, z, across, along] = self.frame;
        let place = |offset: f32, share: f32, count: usize| {
            let at = (offset * share * count as f32).floor();
            (0.0..count as f32).contains(&at).then_some(at as usize)
        };
        let column = place(at.x - x, across, self.columns)?;
        let row = place(at.z - z, along, self.rows)?;
        Some(row * self.columns + column)
    }

    /// Writes into `texels`, 4 bytes a cell, a seen cell's light in each.
    fn clear_texels(&self, texels: &mut [u8]) {
        let [r, g, b] = self.file.light(self.file.seen);
        for texel in texels.as_chunks_mut::<4>().0 {
            *texel = [r, g, b, u8::MAX];
        }
    }

    /// Writes into `texels`, 4 bytes a cell, each cell's light: a seen one's, an explored one's,
    /// as is each of a map that starts known, or a shrouded one's.
    fn texels(&self, seen: &CellBits, explored: &CellBits, texels: &mut [u8]) {
        let file = &self.file;
        let light = |level: FogLevel| {
            let [r, g, b] = file.light(level);
            [r, g, b, u8::MAX]
        };
        let [clear, fogged, shrouded] = [file.seen, file.explored, file.shrouded].map(light);
        for (cell, texel) in texels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            *texel = if seen.holds(cell) {
                clear
            } else if !self.shroud || explored.holds(cell) {
                fogged
            } else {
                shrouded
            };
        }
    }

    /// The frame of `grid`'s texture: its first cell's corner and the size of its cells.
    fn frame(grid: &VisionGrid) -> [f32; 4] {
        let [x, z] = grid.grid.bounds().min().map(FloatNum::float);
        let cell = grid.grid.cell().float();
        let width = grid.grid.columns() as f32 * cell;
        let depth = grid.grid.rows() as f32 * cell;
        [x, z, width.recip(), depth.recip()]
    }

    /// A texture of `columns` by `rows` texels of `texel`, sampled linearly between them and
    /// clamped at its edges, which the client keeps to write again.
    fn image(columns: usize, rows: usize, texel: [u8; 4]) -> Image {
        let size = Extent3d {
            width: u32::try_from(columns).expect("a grid's columns fit u32"),
            height: u32::try_from(rows).expect("a grid's rows fit u32"),
            depth_or_array_layers: 1,
        };
        let mut image = Image::new_fill(
            size,
            TextureDimension::D2,
            &texel,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::linear());
        image
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use campfire_client_files::FogFile;

    use crate::view::fog::MatchFog;

    impl MatchFog {
        /// A known map's fog over `columns` by `rows` cells of 1 m from the origin.
        pub(crate) fn of_cells(columns: usize, rows: usize) -> Self {
            let size = |count: usize| (count as f32).recip();
            Self {
                file: FogFile::default(),
                columns,
                rows,
                frame: [0.0, 0.0, size(columns), size(rows)],
                shroud: false,
            }
        }
    }
}

#[cfg(test)]
mod tests;
