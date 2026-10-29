use std::collections::BTreeMap;

use bevy::asset::RenderAssetUsages;
use bevy::image::{Image, ImageSampler, ImageSamplerDescriptor};
use bevy::math::{UVec2, Vec2};
use bevy::render::render_resource::{Extent3d, TextureDataOrder, TextureDimension, TextureFormat};
use campfire_client_files::{BorderWrap, TerrainTexture, TextureCopy, TextureLevels};
use campfire_package_format::PackageReader;
use campfire_values::PackagePath;
use tracing::error;

use crate::view::client_data::ClientLoad;

/// A terrain's texture, composed as a terrain file says (02-engine-core.md, The terrain's texture
/// is composed at load): at each level, each copy of a package texture's rectangle in order, then
/// each border wrap. A copy whose texture does not read, holds fewer levels than the terrain, or
/// lacks its rectangle, leaves its place opaque black, logged.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct TerrainImage {
    size: UVec2,
    levels: u32,
    /// Its levels one after another, the largest first, each rows from the top of sRGB red,
    /// green, blue and alpha.
    texels: Vec<u8>,
}

const BLACK: [u8; 4] = [0, 0, 0, 255];

/// One level of a texture as it is composed: its texels, `width` a row, and the level.
#[derive(Debug)]
struct LevelTexels<'t> {
    texels: &'t mut [u8],
    width: usize,
    level: u32,
}

impl TerrainImage {
    /// The texture of `texture`, its copies' textures read from `files`, each once.
    pub(crate) fn compose(texture: &TerrainTexture, files: &PackageReader) -> Self {
        let mut sources = BTreeMap::new();
        for copy in &texture.copies {
            sources
                .entry(&copy.texture)
                .or_insert_with(|| Self::source(files, &copy.texture, texture.levels));
        }
        // In `usize`, as a side of `TerrainTexture::MOST_SIDE` squared passes a `u32`'s texels
        // four bytes each.
        let texels_of = |level: u32| {
            let [width, height] = (texture.size >> level).to_array().map(usize_of);
            width * height
        };
        let mut texels = BLACK.repeat((0..texture.levels).map(texels_of).sum());
        let mut rest = texels.as_mut_slice();
        for level in 0..texture.levels {
            let (this, next) = rest.split_at_mut(texels_of(level) * 4);
            let mut target = LevelTexels {
                texels: this,
                width: usize_of(texture.size.x >> level),
                level,
            };
            for copy in &texture.copies {
                Self::copy(&mut target, copy, sources[&copy.texture].as_ref());
            }
            for wrap in &texture.wraps {
                Self::wrap(&mut target, *wrap);
            }
            rest = next;
        }
        Self {
            size: texture.size,
            levels: texture.levels,
            texels,
        }
    }

    /// The color of the texel of its first level at `uv`, in texels, `u` right and `v` down,
    /// the nearest one within it.
    #[expect(
        clippy::cast_sign_loss,
        reason = "a place is cast once it is clamped at 0 or more"
    )]
    pub(crate) fn texel(&self, uv: Vec2) -> [u8; 3] {
        let [width, height] = [self.size.x, self.size.y].map(|side| side.max(1) as f32);
        let column = uv.x.clamp(0.0, width - 1.0) as usize;
        let row = uv.y.clamp(0.0, height - 1.0) as usize;
        let at = (row * self.size.x as usize + column) * 4;
        [self.texels[at], self.texels[at + 1], self.texels[at + 2]]
    }

    /// The texture as Bevy draws it, sampled linearly between texels and levels.
    pub(crate) fn into_image(self) -> Image {
        let size = Extent3d {
            width: self.size.x,
            height: self.size.y,
            depth_or_array_layers: 1,
        };
        let mut image = Image::new_uninit(
            size,
            TextureDimension::D2,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        );
        image.texture_descriptor.mip_level_count = self.levels;
        image.data_order = TextureDataOrder::MipMajor;
        image.data = Some(self.texels);
        image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::linear());
        image
    }

    /// The first `levels` levels of the texture at `path`; none, logged, for one that does not
    /// read.
    fn source(files: &PackageReader, path: &PackagePath, levels: u32) -> Option<TextureLevels> {
        let bytes = ClientLoad::logged(path, files.read_file(path))?;
        ClientLoad::logged(path, TextureLevels::of_ktx2(&bytes, levels))
    }

    /// Copies `copy`'s rectangle of `source` at the target's level into `target`; none, logged,
    /// when the source has no texture or lacks the rectangle.
    fn copy(target: &mut LevelTexels<'_>, copy: &TextureCopy, source: Option<&TextureLevels>) {
        let LevelTexels {
            texels,
            width,
            level,
        } = target;
        let (width, level) = (*width, *level);
        let holds = |source: &TextureLevels| {
            let end = copy.from.checked_add(copy.size);
            end.is_some_and(|end| end.cmple(source.size()).all())
        };
        let Some(source) = source.filter(|&source| holds(source)) else {
            if level == 0 && source.is_some() {
                error!(path = %copy.texture, from = %copy.from, size = %copy.size, "a terrain's texture lacks a copy's rectangle");
            }
            return;
        };
        let from = texels_at(copy.from >> level);
        let to = texels_at(copy.to >> level);
        let size = texels_at(copy.size >> level);
        let side = (source.size().x >> level).max(1) as usize;
        let data = source.level(level);
        for row in 0..size.y {
            let from = ((from.y + row) * side + from.x) * 4;
            let to = ((to.y + row) * width + to.x) * 4;
            texels[to..to + size.x * 4].copy_from_slice(&data[from..from + size.x * 4]);
        }
    }

    /// Wraps `wrap`'s border at the target's level in `target`: its columns from the rectangle's
    /// opposite side, then its rows, each the border's full width.
    fn wrap(target: &mut LevelTexels<'_>, wrap: BorderWrap) {
        let LevelTexels {
            texels,
            width,
            level,
        } = target;
        let (width, level) = (*width, *level);
        let Texels { x, y } = texels_at(wrap.at >> level);
        let Texels {
            x: columns,
            y: rows,
        } = texels_at(wrap.size >> level);
        let border = (wrap.border >> level) as usize;
        let texel = |column: usize, row: usize| (row * width + column) * 4;
        for row in y..y + rows {
            texels.copy_within(
                texel(x + columns - border, row)..texel(x + columns, row),
                texel(x - border, row),
            );
            texels.copy_within(
                texel(x, row)..texel(x + border, row),
                texel(x + columns, row),
            );
        }
        let full = |row: usize| texel(x - border, row)..texel(x + columns + border, row);
        for line in 0..border {
            texels.copy_within(full(y + rows - 1 - line), texel(x - border, y - 1 - line));
            texels.copy_within(full(y + line), texel(x - border, y + rows + line));
        }
    }
}

/// A texel position or size as indices into a texture's rows.
#[derive(Debug, Clone, Copy)]
struct Texels {
    x: usize,
    y: usize,
}

/// `at` as indices: a texture's side fits `usize` on every target.
fn texels_at(at: UVec2) -> Texels {
    let index = |value: u32| usize::try_from(value).expect("a texel index fits usize");
    Texels {
        x: index(at.x),
        y: index(at.y),
    }
}

/// A texture's side as a length.
fn usize_of(side: u32) -> usize {
    usize::try_from(side).expect("a side fits usize")
}

#[cfg(test)]
pub(crate) mod internals {
    use bevy::math::UVec2;

    use crate::view::terrain_image::TerrainImage;

    impl TerrainImage {
        /// A texture of one level of `size` texels, `texels` of 4 bytes each, row by row.
        pub(crate) fn of_texels(size: UVec2, texels: Vec<u8>) -> Self {
            Self {
                size,
                levels: 1,
                texels,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use campfire_store::internals::TestDir;

    use super::*;
    use crate::view::package_source::internals::package;

    /// A KTX2 file of `side` × `side` texels at three levels, each texel its column, its row and
    /// its level, opaque.
    fn numbered(side: u32) -> Vec<u8> {
        let mut bytes = TextureLevels::ktx2(UVec2::splat(side), 3, |_| [0; 4]);
        let data: Vec<u8> = (0..3_u32)
            .rev()
            .flat_map(|level| {
                let side = side >> level;
                (0..side).flat_map(move |y| {
                    (0..side).flat_map(move |x| [byte(x), byte(y), byte(level), 255])
                })
            })
            .collect();
        let start = bytes.len() - data.len();
        bytes[start..].copy_from_slice(&data);
        bytes
    }

    fn byte(value: u32) -> u8 {
        u8::try_from(value).unwrap()
    }

    /// The texel `[x, y]` of level `level`, after the levels above it.
    fn texel(image: &TerrainImage, level: u32, [x, y]: [usize; 2]) -> [u8; 4] {
        let start: usize = (0..level)
            .map(|above| (image.size >> above).element_product() as usize * 4)
            .sum();
        let at = start + (y * (image.size.x >> level) as usize + x) * 4;
        image.texels[at..at + 4].try_into().unwrap()
    }

    #[test]
    fn copies_fill_their_places_at_each_level_their_borders_wrap_and_a_missing_one_is_black() {
        let test_dir = TestDir::new();
        let files = package(
            &test_dir,
            "mode",
            &[("client/textures/sand.ktx2", &numbered(16))],
        );
        let path = |name: &str| PackagePath::parse(name).unwrap();
        // A texture of 32 × 16 texels at two levels: the sand's 8 × 8 texels from (8, 4) to
        // (4, 4), wrapped by 4 texels; a missing texture's 8 × 8 to (20, 4), unwrapped.
        let texture = TerrainTexture {
            size: UVec2::new(32, 16),
            levels: 2,
            copies: vec![
                TextureCopy {
                    texture: path("client/textures/sand.ktx2"),
                    from: UVec2::new(8, 4),
                    to: UVec2::new(4, 4),
                    size: UVec2::splat(8),
                },
                TextureCopy {
                    texture: path("client/textures/gone.ktx2"),
                    from: UVec2::ZERO,
                    to: UVec2::new(20, 4),
                    size: UVec2::splat(8),
                },
            ],
            wraps: vec![BorderWrap {
                at: UVec2::new(4, 4),
                size: UVec2::splat(8),
                border: 4,
            }],
        };
        let image = TerrainImage::compose(&texture, &files);
        assert_eq!(image.levels, 2);
        assert_eq!(image.texels.len(), (32 * 16 + 16 * 8) * 4);
        // The copy's corner (4, 4) is the sand's (8, 4), and (11, 11) its (15, 11).
        assert_eq!(texel(&image, 0, [4, 4]), [8, 4, 0, 255]);
        assert_eq!(texel(&image, 0, [11, 11]), [15, 11, 0, 255]);
        // Left of column 4 is the block's last column, the sand's 15; right of its last, its
        // first, 8; above row 4 its last row, 11; a corner takes the opposite corner.
        assert_eq!(texel(&image, 0, [3, 6]), [15, 6, 0, 255]);
        assert_eq!(texel(&image, 0, [0, 6]), [12, 6, 0, 255]);
        assert_eq!(texel(&image, 0, [12, 6]), [8, 6, 0, 255]);
        assert_eq!(texel(&image, 0, [9, 3]), [13, 11, 0, 255]);
        assert_eq!(texel(&image, 0, [9, 12]), [13, 4, 0, 255]);
        assert_eq!(texel(&image, 0, [0, 0]), [12, 8, 0, 255]);
        assert_eq!(texel(&image, 0, [15, 15]), [11, 7, 0, 255]);
        // Level 1 halves it all: the sand's level 1 from (4, 2) to (2, 2), a border of 2.
        assert_eq!(texel(&image, 1, [2, 2]), [4, 2, 1, 255]);
        assert_eq!(texel(&image, 1, [5, 5]), [7, 5, 1, 255]);
        assert_eq!(texel(&image, 1, [1, 3]), [7, 3, 1, 255]);
        assert_eq!(texel(&image, 1, [3, 0]), [5, 4, 1, 255]);
        // The missing texture's place, and all no copy or border fills, stay opaque black.
        assert_eq!(texel(&image, 0, [20, 4]), BLACK);
        assert_eq!(texel(&image, 0, [27, 11]), BLACK);
        assert_eq!(texel(&image, 1, [10, 2]), BLACK);
        assert_eq!(texel(&image, 0, [16, 0]), BLACK);
        // A copy past its texture's 16 texels leaves its place black too.
        let mut past = texture;
        past.copies[0].from = UVec2::new(12, 4);
        let image = TerrainImage::compose(&past, &files);
        assert_eq!(texel(&image, 0, [4, 4]), BLACK);
    }
}
