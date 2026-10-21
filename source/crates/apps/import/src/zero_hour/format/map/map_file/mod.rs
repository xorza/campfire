use std::borrow::Cow;
use std::collections::BTreeMap;

use miniz_oxide::inflate::decompress_to_vec_zlib_with_limit;

use crate::zero_hour::format::look_conditions::LookConditions;
use crate::zero_hour::format::map::chunk_reader::{ChunkReader, RawChunk};
use crate::zero_hour::format::map::error::{Chunk, MapReadError, Packing};
use crate::zero_hour::format::map::map_dict::{self, DictValue};
use crate::zero_hour::format::map::map_object::MapObject;
use crate::zero_hour::format::map::terrain::{
    BlendShape, BlendTile, CliffUv, TerrainCell, TerrainFields, TextureClass,
};
use crate::zero_hour::format::ref_pack;

/// A map as the game reads its `.map` file, `WorldHeightMap`'s chunks: its heights, its
/// terrain's tiles and blends, the objects the game keeps, in the file's order, and its objects'
/// lights.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ZeroHourMap {
    pub(crate) heights: MapHeights,
    /// Its cells in the file's order, one for each sample of the heights.
    pub(crate) terrain: TerrainFields,
    pub(crate) objects: Vec<MapObject>,
    /// The three lights its objects are drawn in at its time of day (`GlobalLighting`), on the
    /// game's axes, a light the file does not give as the game starts it, dark and straight down.
    pub(crate) lights: [GameLight; 3],
    /// The conditions its models follow: night at a time of day of night, snow when its world
    /// dictionary's `weather` is snowy.
    pub(crate) conditions: LookConditions,
}

/// A map's `GlobalLighting` as the import reads it: the lights of its time of day, and whether
/// that time is night.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Lighting {
    lights: [GameLight; 3],
    night: bool,
}

/// `TIME_OF_DAY_NIGHT`, the last of the four.
const NIGHT: i32 = 4;

/// `WEATHER_SNOWY`.
const SNOWY: i32 = 1;

/// One of the game's global lights: its ambient and diffuse colors, and the way its light goes,
/// which the game calls its position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GameLight {
    pub(crate) ambient: [f32; 3],
    pub(crate) diffuse: [f32; 3],
    pub(crate) position: [f32; 3],
}

impl GameLight {
    /// The light the game starts each with.
    const DARK: Self = Self {
        ambient: [0.0; 3],
        diffuse: [0.0; 3],
        position: [0.0, 0.0, -1.0],
    };
}

/// A map's heights: one byte for each sample, row after row in the file's order, each row
/// `width` samples, and the samples of its border on each side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MapHeights {
    pub(crate) width: usize,
    pub(crate) depth: usize,
    pub(crate) border: usize,
    /// The first playable boundary, in cells, the one a map starts with active; none when the
    /// file holds none.
    pub(crate) boundary: Option<[i32; 2]>,
    pub(crate) samples: Vec<u8>,
}

/// The mark that ends each blended tile (`FLAG_VAL`).
const BLEND_MARK: u32 = 0x7ADA_0000;

/// The heights' span past which a cell of a map that stores no cliff bits is a cliff:
/// `PATHFIND_CLIFF_SLOPE_LIMIT_F`, 9.8, lies between 15 and 16 steps of 0.625.
const CLIFF_STEPS: u8 = 16;

/// Which list of a map's texture classes a class is of: the terrain's, which holds one more
/// field, or the edges'.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClassForm {
    Terrain,
    Edge,
}

impl ZeroHourMap {
    /// The map whose file, packed or not, is `packed`.
    pub(crate) fn read(packed: &[u8]) -> Result<Self, MapReadError> {
        let bytes = Self::unpack(packed)?;
        let mut reader = ChunkReader::new(&bytes);
        if reader.cursor.take(4) != Ok(b"CkMp") {
            return Err(MapReadError::NotChunks);
        }
        let mut names = BTreeMap::new();
        for _ in 0..reader.count()? {
            let len = reader.cursor.u8()?;
            let name = reader.cursor.take(len.into())?;
            let id = reader.cursor.u32()?;
            if names.insert(id, name).is_some() {
                return Err(MapReadError::NameTwice(id));
            }
        }
        let mut chunks: [Option<RawChunk<'_>>; 5] = [const { None }; 5];
        let kinds = [
            Chunk::HeightMapData,
            Chunk::BlendTileData,
            Chunk::ObjectsList,
            Chunk::GlobalLighting,
            Chunk::WorldInfo,
        ];
        while !reader.is_empty() {
            let chunk = reader.chunk()?;
            let name = *names
                .get(&chunk.id)
                .ok_or(MapReadError::UnknownName(chunk.id))?;
            let Some(at) = kinds.iter().position(|kind| kind.name().as_bytes() == name) else {
                continue;
            };
            if chunks[at].replace(chunk).is_some() {
                return Err(MapReadError::Twice(kinds[at]));
            }
        }
        let [heights, terrain, objects, lighting, world] = chunks;
        let heights = Self::heights(heights.ok_or(MapReadError::Missing(Chunk::HeightMapData))?)?;
        let terrain = Self::terrain(
            terrain.ok_or(MapReadError::Missing(Chunk::BlendTileData))?,
            &heights,
        )?;
        let objects = Self::objects(
            objects.ok_or(MapReadError::Missing(Chunk::ObjectsList))?,
            &names,
        )?;
        let lighting =
            Self::lighting(lighting.ok_or(MapReadError::Missing(Chunk::GlobalLighting))?)?;
        let snow = match world {
            Some(world) => Self::snowy(world, &names)?,
            None => false,
        };
        Ok(Self {
            heights,
            terrain,
            objects,
            lights: lighting.lights,
            conditions: LookConditions {
                night: lighting.night,
                snow,
            },
        })
    }

    /// The file's bytes, unpacked as `CompressionManager` unpacks them: by the tag of its first
    /// four bytes, past an 8-byte header that ends with the unpacked size, and as they are under
    /// no tag the game knows.
    fn unpack(packed: &[u8]) -> Result<Cow<'_, [u8]>, MapReadError> {
        let (Some(tag), Some(size), Some(stream)) =
            (packed.get(..4), packed.get(4..8), packed.get(8..))
        else {
            return Ok(Cow::Borrowed(packed));
        };
        let len = usize::try_from(u32::from_le_bytes(size.try_into().expect("four bytes")))
            .expect("a u32 fits usize");
        let unpacked = match tag {
            b"EAR\0" => ref_pack::decode(stream).map_err(MapReadError::RefPack)?,
            [b'Z', b'L', b'1'..=b'9', 0] => decompress_to_vec_zlib_with_limit(stream, len)
                .map_err(|error| MapReadError::Zlib(error.status))?,
            b"NOX\0" => return Err(MapReadError::Packing(Packing::Nox)),
            b"EAB\0" => return Err(MapReadError::Packing(Packing::BTree)),
            b"EAH\0" => return Err(MapReadError::Packing(Packing::Huffman)),
            _ => return Ok(Cow::Borrowed(packed)),
        };
        if unpacked.len() != len {
            return Err(MapReadError::PackedSize {
                written: unpacked.len(),
                len,
            });
        }
        Ok(Cow::Owned(unpacked))
    }

    /// `HeightMapData` version 4 (`WorldHeightMap::ParseHeightMapData`): its sizes, its border,
    /// its playable boundaries, of which the import keeps the first, and its samples.
    fn heights(chunk: RawChunk<'_>) -> Result<MapHeights, MapReadError> {
        chunk.check_version(Chunk::HeightMapData, 4..=4)?;
        let mut body = chunk.body;
        let width = body.count()?;
        let depth = body.count()?;
        let border = body.count()?;
        let boundaries = body.count()?;
        let mut kept = body
            .cursor
            .take(boundaries.checked_mul(8).ok_or(MapReadError::Short)?)?
            .as_chunks::<4>()
            .0
            .iter()
            .map(|&bytes| i32::from_le_bytes(bytes));
        let boundary = kept.next().zip(kept.next()).map(|(x, y)| [x, y]);
        let len = body.count()?;
        if len == 0 || Some(len) != width.checked_mul(depth) {
            return Err(MapReadError::Samples { len, width, depth });
        }
        let samples = body.cursor.take(len)?.to_vec();
        body.ended(Chunk::HeightMapData)?;
        Ok(MapHeights {
            width,
            depth,
            border,
            boundary,
            samples,
        })
    }

    /// `BlendTileData` versions 6 to 8 (`WorldHeightMap::ParseBlendTileData`): each sample's
    /// tile, blend, third blend and cliff UVs; its cliff bits, which version 6 lacks and the
    /// game makes from the heights, and which version 7 stored rows of `(width + 1) / 8` bytes
    /// of; the texture classes, the edge classes, the blended tiles and the cliff UVs, each list
    /// past an unused entry 0 that an index of 0 names.
    fn terrain(chunk: RawChunk<'_>, heights: &MapHeights) -> Result<TerrainFields, MapReadError> {
        chunk.check_version(Chunk::BlendTileData, 6..=8)?;
        let version = chunk.version;
        let mut body = chunk.body;
        let len = body.count()?;
        if len != heights.samples.len() {
            return Err(MapReadError::Samples {
                len,
                width: heights.width,
                depth: heights.depth,
            });
        }
        let tiles = body.i16s(len)?;
        let blends = body.i16s(len)?;
        let extra_blends = body.i16s(len)?;
        let cliff_uvs = body.i16s(len)?;
        let cliffs = if version == 6 {
            Self::cliffs_of_heights(heights)
        } else {
            let row = if version == 7 {
                (heights.width + 1) / 8
            } else {
                heights.width.div_ceil(8)
            };
            let bytes = body
                .cursor
                .take(row.checked_mul(heights.depth).ok_or(MapReadError::Short)?)?;
            (0..len)
                .map(|at| {
                    let (y, x) = (at / heights.width, at % heights.width);
                    x / 8 < row && bytes[y * row + x / 8] & (1 << (x % 8)) != 0
                })
                .collect()
        };
        let tile_count = body.non_negative()?;
        let blend_count = body.count()?;
        let cliff_count = body.count()?;
        let classes = (0..body.count()?)
            .map(|_| Self::texture_class(&mut body, ClassForm::Terrain))
            .collect::<Result<_, _>>()?;
        let edge_tiles = body.non_negative()?;
        let edge_classes = (0..body.count()?)
            .map(|_| Self::texture_class(&mut body, ClassForm::Edge))
            .collect::<Result<_, _>>()?;
        let blend_tiles = (1..blend_count)
            .map(|at| Self::blend_tile(&mut body, at))
            .collect::<Result<_, _>>()?;
        let cliff_tiles = (1..cliff_count)
            .map(|_| Self::cliff_uv(&mut body))
            .collect::<Result<_, _>>()?;
        body.ended(Chunk::BlendTileData)?;
        let cells = (0..len)
            .map(|at| {
                Ok(TerrainCell {
                    tile: index(tiles[at])?,
                    blend: entry(blends[at])?,
                    extra_blend: entry(extra_blends[at])?,
                    cliff_uv: entry(cliff_uvs[at])?,
                    cliff: cliffs[at],
                })
            })
            .collect::<Result<_, MapReadError>>()?;
        Ok(TerrainFields {
            cells,
            tiles: tile_count,
            classes,
            edge_tiles,
            edge_classes,
            blends: blend_tiles,
            cliffs: cliff_tiles,
        })
    }

    /// The cliff bits `initCliffFlagsFromHeights` gives: a cell is a cliff when its four
    /// corners' heights span more than `CLIFF_STEPS` − 1 steps; the last row and column are none.
    fn cliffs_of_heights(heights: &MapHeights) -> Vec<bool> {
        let MapHeights { width, samples, .. } = heights;
        (0..samples.len())
            .map(|at| {
                let (y, x) = (at / width, at % width);
                if x + 1 == *width || y + 1 == heights.depth {
                    return false;
                }
                let corners = [at, at + 1, at + width, at + width + 1].map(|at| samples[at]);
                let low = corners.iter().min().expect("four corners");
                let high = corners.iter().max().expect("four corners");
                high - low >= CLIFF_STEPS
            })
            .collect()
    }

    /// A texture class of `form`: its first tile, its count of tiles, its width in tiles, a
    /// terrain class's field the game no longer reads, and its name, by which the conversion finds
    /// its texture.
    fn texture_class(
        body: &mut ChunkReader<'_>,
        form: ClassForm,
    ) -> Result<TextureClass, MapReadError> {
        let first_tile = body.non_negative()?;
        let tiles = body.non_negative()?;
        let width = body.non_negative()?;
        if form == ClassForm::Terrain {
            body.cursor.i32()?;
        }
        let name = body.text()?;
        Ok(TextureClass {
            first_tile,
            tiles,
            width,
            name: String::from_utf8(name.to_vec())
                .ok()
                .filter(|name| name.is_ascii())
                .ok_or(MapReadError::NotAscii)?,
        })
    }

    /// The blended tile at `at` of the file's list: its tile, its four direction bytes, of
    /// which exactly one is set, its inverted bits, its long diagonal, its custom edge class or
    /// −1, and the mark that ends it.
    fn blend_tile(body: &mut ChunkReader<'_>, at: usize) -> Result<BlendTile, MapReadError> {
        let tile = index(short_index(body.cursor.i32()?)?)?;
        let [horizontal, vertical, right, left, inverted, long] =
            body.cursor.take(6)?.try_into().expect("six bytes");
        let long = long != 0;
        let shape = match [horizontal, vertical, right, left].map(|set| set != 0) {
            [true, false, false, false] => BlendShape::Horizontal,
            [false, true, false, false] => BlendShape::Vertical,
            [false, false, true, false] => BlendShape::RightDiagonal { long },
            [false, false, false, true] => BlendShape::LeftDiagonal { long },
            _ => return Err(MapReadError::BlendShape(at)),
        };
        if inverted & !0b11 != 0 {
            return Err(MapReadError::BlendInverted(at));
        }
        let custom_edge = match body.cursor.i32()? {
            -1 => None,
            edge => Some(index(short_index(edge)?)?),
        };
        if body.cursor.u32()? != BLEND_MARK {
            return Err(MapReadError::BlendMark(at));
        }
        Ok(BlendTile {
            tile,
            shape,
            inverted: inverted & 1 != 0,
            flipped: inverted & 2 != 0,
            custom_edge,
        })
    }

    /// Cliff UVs: its tile, a `[u, v]` for each corner, and two bytes the game reads as bools.
    fn cliff_uv(body: &mut ChunkReader<'_>) -> Result<CliffUv, MapReadError> {
        let tile = index(short_index(body.cursor.i32()?)?)?;
        let mut corners = [[0.0; 2]; 4];
        for corner in &mut corners {
            *corner = [body.cursor.f32()?, body.cursor.f32()?];
        }
        Ok(CliffUv {
            tile,
            corners,
            flip: body.cursor.u8()? != 0,
            mutant: body.cursor.u8()? != 0,
        })
    }

    /// `GlobalLighting` versions 1 to 3 (`WorldHeightMap::ParseLightingDataChunk`): the map's time
    /// of day, then for each of the four, the first terrain light and the first object light, from
    /// version 2 the two other object lights, from version 3 the two other terrain lights; and the
    /// shadows' color, which the import does not keep. The object lights of the map's time of day:
    /// the terrain is lit by them (02-engine-core.md, The terrain is lit as every model is), so its
    /// own are read and dropped, each still refused when a number of it is not finite.
    fn lighting(chunk: RawChunk<'_>) -> Result<Lighting, MapReadError> {
        chunk.check_version(Chunk::GlobalLighting, 1..=3)?;
        let version = chunk.version;
        let mut body = chunk.body;
        let time = body.cursor.i32()?;
        let at = usize::try_from(time)
            .ok()
            .filter(|time| (1..=4).contains(time))
            .ok_or(MapReadError::TimeOfDay(time))?
            - 1;
        let mut times = [[GameLight::DARK; 3]; 4];
        for lights in &mut times {
            Self::light(&mut body)?;
            lights[0] = Self::light(&mut body)?;
            if version >= 2 {
                for light in &mut lights[1..] {
                    *light = Self::light(&mut body)?;
                }
            }
            if version >= 3 {
                for _ in 1..3 {
                    Self::light(&mut body)?;
                }
            }
        }
        if !body.is_empty() {
            body.cursor.u32()?;
        }
        body.ended(Chunk::GlobalLighting)?;
        Ok(Lighting {
            lights: times[at],
            night: time == NIGHT,
        })
    }

    /// Whether the map's `WorldInfo` dictionary names snowy weather, as
    /// `WorldHeightMap::ParseWorldDictDataChunk` sets the game's weather from its `weather`
    /// integer; its keys named by `names`. Its one version is the world builder's
    /// `K_WORLDDICT_VERSION_1`.
    fn snowy(chunk: RawChunk<'_>, names: &BTreeMap<u32, &[u8]>) -> Result<bool, MapReadError> {
        chunk.check_version(Chunk::WorldInfo, 1..=1)?;
        let mut body = chunk.body;
        let mut weather = None;
        map_dict::read(&mut body, names, |key, value| {
            if key == b"weather" {
                weather = match value {
                    DictValue::Int(value) => Some(value),
                    DictValue::Text(_) | DictValue::Other => None,
                };
            }
        })?;
        body.ended(Chunk::WorldInfo)?;
        Ok(weather == Some(SNOWY))
    }

    /// A light: its ambient color, its diffuse color and its position, each three floats.
    fn light(body: &mut ChunkReader<'_>) -> Result<GameLight, MapReadError> {
        let mut values = [0.0; 9];
        for value in &mut values {
            *value = body.cursor.f32()?;
        }
        if !values.iter().all(|value| value.is_finite()) {
            return Err(MapReadError::Lighting);
        }
        let three = |from: usize| [values[from], values[from + 1], values[from + 2]];
        Ok(GameLight {
            ambient: three(0),
            diffuse: three(3),
            position: three(6),
        })
    }

    /// `ObjectsList` version 3: its `Object` chunks of version 3, each kept as the game keeps
    /// it; a chunk of another name is skipped, as the game has no parser for it.
    fn objects(
        chunk: RawChunk<'_>,
        names: &BTreeMap<u32, &[u8]>,
    ) -> Result<Vec<MapObject>, MapReadError> {
        chunk.check_version(Chunk::ObjectsList, 3..=3)?;
        let mut body = chunk.body;
        let mut objects = Vec::new();
        while !body.is_empty() {
            let mut object = body.chunk()?;
            if *names
                .get(&object.id)
                .ok_or(MapReadError::UnknownName(object.id))?
                != b"Object"
            {
                continue;
            }
            object.check_version(Chunk::Object, 3..=3)?;
            let read = MapObject::read(&mut object.body, names)?;
            object.body.ended(Chunk::Object)?;
            if read.is_kept() {
                objects.push(read);
            }
        }
        Ok(objects)
    }
}

/// An `i32` index the game keeps in an `i16`.
fn short_index(value: i32) -> Result<i16, MapReadError> {
    i16::try_from(value).map_err(|_| MapReadError::Index(value))
}

fn index(value: i16) -> Result<u16, MapReadError> {
    u16::try_from(value).map_err(|_| MapReadError::Index(value.into()))
}

/// An index into a list past its unused entry 0: `None` for 0.
fn entry(value: i16) -> Result<Option<u16>, MapReadError> {
    Ok(index(value)?.checked_sub(1))
}

#[cfg(test)]
pub(crate) mod samples;

#[cfg(test)]
mod tests;
