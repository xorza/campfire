use bevy_math::{UVec2, Vec2};
use campfire_common::MapName;
use campfire_values::PackagePath;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::client_path;
use crate::error::TerrainFileError;

/// A map's terrain, `client/maps/<name>/terrain.bin`, through `Binary` (02-engine-core.md, Client):
/// a ground quad for each cell of the map's height grid and the overlays drawn over them, each with
/// its own coordinates in its texture; the texture, composed of package textures as the client
/// loads it; and the surface it is lit as. It names no rule of a game.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TerrainFile {
    quads: TerrainQuads,
    texture: TerrainTexture,
    surface: TerrainSurface,
}

/// The quads over a height grid of `columns` samples a row: a ground quad for each of its cells,
/// rows from the grid's first, and the overlays, in drawing order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerrainQuads {
    pub columns: u32,
    pub ground: Vec<GroundQuad>,
    pub overlays: Vec<Overlay>,
}

/// A cell's ground: the coordinates of its corners in the texture, in texels of its first level,
/// `u` right and `v` down, its corners at the cell's samples `(column, row)`, `(column + 1, row)`,
/// `(column + 1, row + 1)` and `(column, row + 1)`; and whether its two triangles meet on the
/// diagonal from its corner 1 to 3, not from 0 to 2.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GroundQuad {
    pub uv: [Vec2; 4],
    pub flip: bool,
}

/// A quad over a cell, by its index among the ground's, blended by its alpha at each corner over
/// what is drawn before it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Overlay {
    pub cell: u32,
    pub uv: [Vec2; 4],
    pub alpha: [u8; 4],
    pub flip: bool,
}

/// The terrain's texture: `size` texels at its first level, `levels` levels, each half the one
/// above; its copies, each in order, then its border wraps, at each level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerrainTexture {
    pub size: UVec2,
    pub levels: u32,
    pub copies: Vec<TextureCopy>,
    pub wraps: Vec<BorderWrap>,
}

/// A rectangle of `size` texels of a package texture from `from` in it, rows from its top,
/// copied to `to` in the terrain's texture; at a level below, each halved and rounded down, from
/// that level of its texture. `to` and `size` lie on texels every level halves to; `from` may
/// lie on any, as its texture's own levels round down alike.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextureCopy {
    pub texture: PackagePath,
    pub from: UVec2,
    pub to: UVec2,
    pub size: UVec2,
}

/// A border `border` texels wide around the rectangle of `size` texels at `at` in the terrain's
/// texture, its columns and then its rows wrapped from the rectangle's opposite side, so a
/// filtered or a stretched sample near its edge reads its own rectangle; at a level below, each
/// halved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BorderWrap {
    pub at: UVec2,
    pub size: UVec2,
    pub border: u32,
}

/// The terrain's surface, as a glTF material's: its metallic, roughness and specular factors, each
/// from 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TerrainSurface {
    pub metallic: f32,
    pub roughness: f32,
    pub specular: f32,
}

impl TerrainFile {
    /// The terrain of `quads`, `texture` and `surface`; an error unless its ground quads fill
    /// whole rows of its grid's cells, every overlay names one of them, every coordinate is
    /// finite, every copy and border wrap lies within the texture, each place and size on texels
    /// each level halves to, and every factor of its surface is from 0 to 1.
    pub fn new(
        quads: TerrainQuads,
        texture: TerrainTexture,
        surface: TerrainSurface,
    ) -> Result<Self, TerrainFileError> {
        let cells = quads.columns.checked_sub(1).filter(|&cells| cells > 0);
        let Some(cells) = cells.map(|cells| cells as usize) else {
            return Err(TerrainFileError::Shape);
        };
        if quads.ground.is_empty() || !quads.ground.len().is_multiple_of(cells) {
            return Err(TerrainFileError::Shape);
        }
        if let Some(at) = quads
            .overlays
            .iter()
            .position(|overlay| overlay.cell as usize >= quads.ground.len())
        {
            return Err(TerrainFileError::Cell(at));
        }
        let finite = |uv: &[Vec2; 4]| uv.iter().all(|corner| corner.is_finite());
        let uvs = quads.ground.iter().map(|quad| &quad.uv);
        if !uvs
            .chain(quads.overlays.iter().map(|overlay| &overlay.uv))
            .all(finite)
        {
            return Err(TerrainFileError::Coordinate);
        }
        texture.check()?;
        let factors = [surface.metallic, surface.roughness, surface.specular];
        if !factors.iter().all(|factor| (0.0..=1.0).contains(factor)) {
            return Err(TerrainFileError::Surface);
        }
        Ok(Self {
            quads,
            texture,
            surface,
        })
    }

    pub const fn quads(&self) -> &TerrainQuads {
        &self.quads
    }

    pub const fn texture(&self) -> &TerrainTexture {
        &self.texture
    }

    pub const fn surface(&self) -> TerrainSurface {
        self.surface
    }

    /// The file of the map `map`.
    pub fn path(map: &MapName) -> PackagePath {
        client_path::of_map(map, "terrain.bin")
    }
}

impl TerrainQuads {
    /// Whether its ground quads are the cells of a height grid of `columns` and `rows` samples:
    /// its columns are the grid's, and it has a ground quad for each cell.
    pub const fn fits(&self, columns: u32, rows: u32) -> bool {
        let cells = (columns as usize - 1) * (rows as usize - 1);
        self.columns == columns && self.ground.len() == cells
    }
}

impl TerrainTexture {
    /// An error unless it has a texel and a level, its sides halve to a texel at each level, and
    /// every copy and border wrap lies within it, each place and size on texels each level halves
    /// to.
    fn check(&self) -> Result<(), TerrainFileError> {
        let [width, height] = self.size.to_array();
        let last = self
            .levels
            .checked_sub(1)
            .ok_or(TerrainFileError::Texture)?;
        let halved = |side: u32| side.checked_shr(last).is_some_and(|side| side > 0);
        if !halved(width) || !halved(height) {
            return Err(TerrainFileError::Texture);
        }
        let step = 1_u32 << last;
        let whole = |values: &[u32]| values.iter().all(|value| value.is_multiple_of(step));
        let within =
            |at: u32, size: u32, side: u32| at.checked_add(size).is_some_and(|end| end <= side);
        for (index, copy) in self.copies.iter().enumerate() {
            let [x, y] = copy.to.to_array();
            let [w, h] = copy.size.to_array();
            let aligned = whole(&[x, y, w, h]);
            if !aligned || w == 0 || h == 0 || !within(x, w, width) || !within(y, h, height) {
                return Err(TerrainFileError::Copy(index));
            }
        }
        for (index, wrap) in self.wraps.iter().enumerate() {
            let [x, y] = wrap.at.to_array();
            let [w, h] = wrap.size.to_array();
            let border = wrap.border;
            let aligned = whole(&[x, y, w, h, border]);
            let outside = |size: u32| size.checked_add(border);
            let room = x >= border
                && y >= border
                && outside(w).is_some_and(|w| within(x, w, width))
                && outside(h).is_some_and(|h| within(y, h, height));
            if !aligned || border > w || border > h || !room {
                return Err(TerrainFileError::Wrap(index));
            }
        }
        Ok(())
    }
}

/// A package's file is untrusted, so a terrain `new` refuses fails to decode.
impl<'de> Deserialize<'de> for TerrainFile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        struct Fields {
            quads: TerrainQuads,
            texture: TerrainTexture,
            surface: TerrainSurface,
        }
        let Fields {
            quads,
            texture,
            surface,
        } = Fields::deserialize(deserializer)?;
        Self::new(quads, texture, surface).map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::Binary;

    use super::*;

    /// A grid of 3 × 2 samples, two cells, with an overlay over the second; a texture of 8 × 8
    /// texels at two levels, of one copy and its border.
    fn parts() -> (TerrainQuads, TerrainTexture, TerrainSurface) {
        let quad = |[left, right]: [f32; 2], flip: bool| GroundQuad {
            uv: [
                Vec2::new(left, 0.0),
                Vec2::new(right, 0.0),
                Vec2::new(right, 1.0),
                Vec2::new(left, 1.0),
            ],
            flip,
        };
        let quads = TerrainQuads {
            columns: 3,
            ground: vec![quad([0.0, 1.0], false), quad([2.0, 3.0], true)],
            overlays: vec![Overlay {
                cell: 1,
                uv: [Vec2::new(4.0, 2.0); 4],
                alpha: [0, 255, 255, 0],
                flip: false,
            }],
        };
        let texture = TerrainTexture {
            size: UVec2::new(8, 8),
            levels: 2,
            copies: vec![TextureCopy {
                texture: PackagePath::parse("client/textures/sand.ktx2").unwrap(),
                from: UVec2::new(0, 2),
                to: UVec2::new(2, 2),
                size: UVec2::new(4, 2),
            }],
            wraps: vec![BorderWrap {
                at: UVec2::new(2, 2),
                size: UVec2::new(4, 2),
                border: 2,
            }],
        };
        let surface = TerrainSurface {
            metallic: 0.0,
            roughness: 1.0,
            specular: 0.0,
        };
        (quads, texture, surface)
    }

    #[test]
    fn a_terrain_reads_back_what_it_writes_and_each_flaw_is_refused() {
        let (quads, texture, surface) = parts();
        let file = TerrainFile::new(quads, texture, surface).unwrap();
        let read: TerrainFile = Binary::decode(&Binary::encode(&file)).unwrap();
        assert_eq!(read, file);
        // Its two cells are those of a grid of 3 × 2 samples, not of 3 × 3 or of 2 × 2.
        assert!(file.quads().fits(3, 2));
        assert!(!file.quads().fits(3, 3));
        assert!(!file.quads().fits(2, 2));
        let refused = |change: fn(&mut TerrainQuads, &mut TerrainTexture, &mut TerrainSurface)| {
            let (mut quads, mut texture, mut surface) = parts();
            change(&mut quads, &mut texture, &mut surface);
            TerrainFile::new(quads, texture, surface).unwrap_err()
        };
        // A grid of one column has no cell; three quads fill no whole rows of two cells.
        assert_eq!(refused(|q, _, _| q.columns = 1), TerrainFileError::Shape);
        assert_eq!(
            refused(|q, _, _| q.ground.push(q.ground[0])),
            TerrainFileError::Shape
        );
        assert_eq!(refused(|q, _, _| q.ground.clear()), TerrainFileError::Shape);
        // An overlay of cell 2, past the two.
        assert_eq!(
            refused(|q, _, _| q.overlays[0].cell = 2),
            TerrainFileError::Cell(0)
        );
        assert_eq!(
            refused(|q, _, _| q.overlays[0].uv[3].y = f32::NAN),
            TerrainFileError::Coordinate
        );
        assert_eq!(
            refused(|q, _, _| q.ground[1].uv[0].x = f32::INFINITY),
            TerrainFileError::Coordinate
        );
        // No level; five levels of a side of 8, which halves to 1 at the fourth and 0 at the fifth.
        assert_eq!(refused(|_, t, _| t.levels = 0), TerrainFileError::Texture);
        assert_eq!(refused(|_, t, _| t.levels = 5), TerrainFileError::Texture);
        // A copy past the texture's 8 texels, of no texels, or of a place or a size off the even
        // texels the second level halves to; its source corner may be on any texel.
        assert_eq!(
            refused(|_, t, _| t.copies[0].to.x = 6),
            TerrainFileError::Copy(0)
        );
        assert_eq!(
            refused(|_, t, _| t.copies[0].size.y = 0),
            TerrainFileError::Copy(0)
        );
        assert_eq!(
            refused(|_, t, _| t.copies[0].to.y = 1),
            TerrainFileError::Copy(0)
        );
        assert_eq!(
            refused(|_, t, _| t.copies[0].size.x = 3),
            TerrainFileError::Copy(0)
        );
        let (quads, mut texture, surface) = parts();
        texture.copies[0].from = UVec2::new(1, 3);
        assert!(TerrainFile::new(quads, texture, surface).is_ok());
        // A border past the texture's edge, wider than its rectangle, or off the even texels.
        assert_eq!(
            refused(|_, t, _| t.wraps[0].at.x = 0),
            TerrainFileError::Wrap(0)
        );
        assert_eq!(
            refused(|_, t, _| t.wraps[0].border = 4),
            TerrainFileError::Wrap(0)
        );
        assert_eq!(
            refused(|_, t, _| t.wraps[0].border = 1),
            TerrainFileError::Wrap(0)
        );
        // A rectangle so wide that its border's end passes `u32`: 2³² − 2 + 2.
        assert_eq!(
            refused(|_, t, _| t.wraps[0].size.x = u32::MAX - 1),
            TerrainFileError::Wrap(0)
        );
        // A factor past 1, or not a number.
        assert_eq!(
            refused(|_, _, s| s.roughness = 1.5),
            TerrainFileError::Surface
        );
        assert_eq!(
            refused(|_, _, s| s.specular = f32::NAN),
            TerrainFileError::Surface
        );
        // A file read is checked as one made is.
        let bytes = Binary::encode(&file);
        let mut bad = file;
        bad.quads.overlays[0].cell = 9;
        assert!(Binary::decode::<TerrainFile>(&Binary::encode(&bad)).is_err());
        // A file is untrusted: each byte cut short is refused, and no flip of one panics.
        for at in 0..bytes.len() {
            let cut = Binary::decode::<TerrainFile>(&bytes[..at]);
            assert!(cut.is_err(), "truncated to {at} bytes");
            for flip in [0x01, 0x80, 0xFF] {
                let mut flawed = bytes.clone();
                flawed[at] ^= flip;
                Binary::decode::<TerrainFile>(&flawed).ok();
            }
        }
    }
}
