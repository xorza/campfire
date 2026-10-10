use std::borrow::Cow;
use std::collections::BTreeMap;
use std::ops::RangeInclusive;

use campfire_package::{BlendShape, BlendTile, CliffUv, TerrainCell, TerrainParts, TextureClass};
use miniz_oxide::inflate::decompress_to_vec_zlib_with_limit;

use crate::zero_hour::chunk_reader::{ChunkReader, RawChunk};
use crate::zero_hour::error::{Chunk, MapError, Packing};
use crate::zero_hour::map_object::MapObject;
use crate::zero_hour::ref_pack::RefPack;

/// A map as the game reads its `.map` file, `WorldHeightMap`'s chunks: its heights, its
/// terrain's tiles and blends, and the objects the game keeps, in the file's order.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MapFile {
    pub(crate) heights: MapHeights,
    /// Its cells in the file's order, one for each sample of the heights.
    pub(crate) terrain: TerrainParts,
    pub(crate) objects: Vec<MapObject>,
    pub(crate) lighting: MapLighting,
}

/// A map's lights for its time of day (`GlobalLighting`): the terrain's and the objects', three
/// each, on the game's axes, a light the file does not give as the game starts it, dark and
/// straight down.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MapLighting {
    pub(crate) terrain: [GameLight; 3],
    pub(crate) objects: [GameLight; 3],
}

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
    const DARK: GameLight = GameLight {
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
    pub(crate) samples: Vec<u8>,
}

/// The mark that ends each blended tile (`FLAG_VAL`).
const BLEND_MARK: u32 = 0x7ADA_0000;

/// The heights' span past which a cell of a map that stores no cliff bits is a cliff:
/// `PATHFIND_CLIFF_SLOPE_LIMIT_F`, 9.8, lies between 15 and 16 steps of 0.625.
const CLIFF_STEPS: u8 = 16;

impl MapFile {
    /// The map whose file, packed or not, is `packed`.
    pub(crate) fn read(packed: &[u8]) -> Result<MapFile, MapError> {
        let bytes = MapFile::unpack(packed)?;
        let mut reader = ChunkReader::new(&bytes);
        if reader.take(4) != Ok(b"CkMp") {
            return Err(MapError::NotChunks);
        }
        let mut names = BTreeMap::new();
        for _ in 0..reader.count()? {
            let len = reader.u8()?;
            let name = reader.take(len.into())?;
            let id = reader.u32()?;
            if names.insert(id, name).is_some() {
                return Err(MapError::NameTwice(id));
            }
        }
        let mut chunks: [Option<RawChunk<'_>>; 4] = [None, None, None, None];
        let kinds = [
            Chunk::HeightMapData,
            Chunk::BlendTileData,
            Chunk::ObjectsList,
            Chunk::GlobalLighting,
        ];
        while !reader.is_empty() {
            let chunk = reader.chunk()?;
            let name = *names
                .get(&chunk.id)
                .ok_or(MapError::UnknownName(chunk.id))?;
            let Some(at) = kinds.iter().position(|kind| kind.name().as_bytes() == name) else {
                continue;
            };
            if chunks[at].replace(chunk).is_some() {
                return Err(MapError::Twice(kinds[at]));
            }
        }
        let [heights, terrain, objects, lighting] = chunks;
        let heights = MapFile::heights(heights.ok_or(MapError::Missing(Chunk::HeightMapData))?)?;
        let terrain = MapFile::terrain(
            terrain.ok_or(MapError::Missing(Chunk::BlendTileData))?,
            &heights,
        )?;
        let objects = MapFile::objects(
            objects.ok_or(MapError::Missing(Chunk::ObjectsList))?,
            &names,
        )?;
        let lighting =
            MapFile::lighting(lighting.ok_or(MapError::Missing(Chunk::GlobalLighting))?)?;
        Ok(MapFile {
            heights,
            terrain,
            objects,
            lighting,
        })
    }

    /// The file's bytes, unpacked as `CompressionManager` unpacks them: by the tag of its first
    /// four bytes, past an 8-byte header that ends with the unpacked size, and as they are under
    /// no tag the game knows.
    fn unpack(packed: &[u8]) -> Result<Cow<'_, [u8]>, MapError> {
        let (Some(tag), Some(size), Some(stream)) =
            (packed.get(..4), packed.get(4..8), packed.get(8..))
        else {
            return Ok(Cow::Borrowed(packed));
        };
        let len = usize::try_from(u32::from_le_bytes(size.try_into().expect("four bytes")))
            .expect("a u32 fits usize");
        let unpacked = match tag {
            b"EAR\0" => RefPack::decode(stream).map_err(MapError::RefPack)?,
            [b'Z', b'L', b'1'..=b'9', 0] => decompress_to_vec_zlib_with_limit(stream, len)
                .map_err(|error| MapError::Zlib(error.status))?,
            b"NOX\0" => return Err(MapError::Packing(Packing::Nox)),
            b"EAB\0" => return Err(MapError::Packing(Packing::BTree)),
            b"EAH\0" => return Err(MapError::Packing(Packing::Huffman)),
            _ => return Ok(Cow::Borrowed(packed)),
        };
        if unpacked.len() != len {
            return Err(MapError::PackedSize {
                written: unpacked.len(),
                len,
            });
        }
        Ok(Cow::Owned(unpacked))
    }

    /// `HeightMapData` version 4 (`WorldHeightMap::ParseHeightMapData`): its sizes, its border,
    /// its playable boundaries, which the import does not keep, and its samples.
    fn heights(chunk: RawChunk<'_>) -> Result<MapHeights, MapError> {
        MapFile::version(&chunk, Chunk::HeightMapData, 4..=4)?;
        let mut body = chunk.body;
        let width = body.count()?;
        let depth = body.count()?;
        let border = body.count()?;
        let boundaries = body.count()?;
        body.take(boundaries.checked_mul(8).ok_or(MapError::Short)?)?;
        let len = body.count()?;
        if len == 0 || Some(len) != width.checked_mul(depth) {
            return Err(MapError::Samples { len, width, depth });
        }
        let samples = body.take(len)?.to_vec();
        MapFile::ended(&body, Chunk::HeightMapData)?;
        Ok(MapHeights {
            width,
            depth,
            border,
            samples,
        })
    }

    /// `BlendTileData` versions 6 to 8 (`WorldHeightMap::ParseBlendTileData`): each sample's
    /// tile, blend, third blend and cliff UVs; its cliff bits, which version 6 lacks and the
    /// game makes from the heights, and which version 7 stored rows of `(width + 1) / 8` bytes
    /// of; the texture classes, the edge classes, the blended tiles and the cliff UVs, each list
    /// past an unused entry 0 that an index of 0 names.
    fn terrain(chunk: RawChunk<'_>, heights: &MapHeights) -> Result<TerrainParts, MapError> {
        MapFile::version(&chunk, Chunk::BlendTileData, 6..=8)?;
        let version = chunk.version;
        let mut body = chunk.body;
        let len = body.count()?;
        if len != heights.samples.len() {
            return Err(MapError::Samples {
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
            MapFile::cliffs_of_heights(heights)
        } else {
            let row = if version == 7 {
                (heights.width + 1) / 8
            } else {
                heights.width.div_ceil(8)
            };
            let bytes = body.take(row.checked_mul(heights.depth).ok_or(MapError::Short)?)?;
            (0..len)
                .map(|at| {
                    let (y, x) = (at / heights.width, at % heights.width);
                    x / 8 < row && bytes[y * row + x / 8] & (1 << (x % 8)) != 0
                })
                .collect()
        };
        let tile_count = MapFile::u32(&mut body)?;
        let blend_count = body.count()?;
        let cliff_count = body.count()?;
        let classes = (0..body.count()?)
            .map(|_| MapFile::texture_class(&mut body, true))
            .collect::<Result<_, _>>()?;
        let edge_tiles = MapFile::u32(&mut body)?;
        let edge_classes = (0..body.count()?)
            .map(|_| MapFile::texture_class(&mut body, false))
            .collect::<Result<_, _>>()?;
        let blend_tiles = (1..blend_count)
            .map(|at| MapFile::blend_tile(&mut body, at))
            .collect::<Result<_, _>>()?;
        let cliff_tiles = (1..cliff_count)
            .map(|_| MapFile::cliff_uv(&mut body))
            .collect::<Result<_, _>>()?;
        MapFile::ended(&body, Chunk::BlendTileData)?;
        let cells = (0..len)
            .map(|at| {
                Ok(TerrainCell {
                    tile: MapFile::index(tiles[at])?,
                    blend: MapFile::entry(blends[at])?,
                    extra_blend: MapFile::entry(extra_blends[at])?,
                    cliff_uv: MapFile::entry(cliff_uvs[at])?,
                    cliff: cliffs[at],
                })
            })
            .collect::<Result<_, MapError>>()?;
        Ok(TerrainParts {
            columns: u32::try_from(heights.width).expect("an i32 count fits u32"),
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

    /// A texture class: its first tile, its count of tiles, its width in tiles, in a texture
    /// class a field the game no longer reads, and its name; its texture is the import's to find.
    fn texture_class(body: &mut ChunkReader<'_>, legacy: bool) -> Result<TextureClass, MapError> {
        let first_tile = MapFile::u32(body)?;
        let tiles = MapFile::u32(body)?;
        let width = MapFile::u32(body)?;
        if legacy {
            body.i32()?;
        }
        let name = body.text()?;
        Ok(TextureClass {
            first_tile,
            tiles,
            width,
            name: String::from_utf8(name.to_vec())
                .ok()
                .filter(|name| name.is_ascii())
                .ok_or(MapError::NotAscii)?,
            texture: None,
        })
    }

    /// The blended tile at `at` of the file's list: its tile, its four direction bytes, of
    /// which exactly one is set, its inverted bits, its long diagonal, its custom edge class or
    /// −1, and the mark that ends it.
    fn blend_tile(body: &mut ChunkReader<'_>, at: usize) -> Result<BlendTile, MapError> {
        let tile = MapFile::index(MapFile::i16(body.i32()?)?)?;
        let [horizontal, vertical, right, left, inverted, long] =
            body.take(6)?.try_into().expect("six bytes");
        let long = long != 0;
        let shape = match [horizontal, vertical, right, left].map(|set| set != 0) {
            [true, false, false, false] => BlendShape::Horizontal,
            [false, true, false, false] => BlendShape::Vertical,
            [false, false, true, false] => BlendShape::RightDiagonal { long },
            [false, false, false, true] => BlendShape::LeftDiagonal { long },
            _ => return Err(MapError::BlendShape(at)),
        };
        if inverted & !0b11 != 0 {
            return Err(MapError::BlendInverted(at));
        }
        let custom_edge = match body.i32()? {
            -1 => None,
            edge => Some(MapFile::index(MapFile::i16(edge)?)?),
        };
        if body.u32()? != BLEND_MARK {
            return Err(MapError::BlendMark(at));
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
    fn cliff_uv(body: &mut ChunkReader<'_>) -> Result<CliffUv, MapError> {
        let tile = MapFile::index(MapFile::i16(body.i32()?)?)?;
        let mut corners = [[0.0; 2]; 4];
        for corner in &mut corners {
            *corner = [body.f32()?, body.f32()?];
        }
        Ok(CliffUv {
            tile,
            corners,
            flip: body.u8()? != 0,
            mutant: body.u8()? != 0,
        })
    }

    /// `GlobalLighting` versions 1 to 3 (`WorldHeightMap::ParseLightingDataChunk`): the map's
    /// time of day, then for each of the four, the first terrain light and the first object
    /// light, from version 2 the two other object lights, from version 3 the two other terrain
    /// lights; and the shadows' color, which the import does not keep. The lights of the map's
    /// time of day.
    fn lighting(chunk: RawChunk<'_>) -> Result<MapLighting, MapError> {
        MapFile::version(&chunk, Chunk::GlobalLighting, 1..=3)?;
        let version = chunk.version;
        let mut body = chunk.body;
        let time = body.i32()?;
        let at = usize::try_from(time)
            .ok()
            .filter(|time| (1..=4).contains(time))
            .ok_or(MapError::TimeOfDay(time))?
            - 1;
        let mut times = [MapLighting {
            terrain: [GameLight::DARK; 3],
            objects: [GameLight::DARK; 3],
        }; 4];
        for lighting in &mut times {
            lighting.terrain[0] = MapFile::light(&mut body)?;
            lighting.objects[0] = MapFile::light(&mut body)?;
            if version >= 2 {
                for light in &mut lighting.objects[1..] {
                    *light = MapFile::light(&mut body)?;
                }
            }
            if version >= 3 {
                for light in &mut lighting.terrain[1..] {
                    *light = MapFile::light(&mut body)?;
                }
            }
        }
        if !body.is_empty() {
            body.u32()?;
        }
        MapFile::ended(&body, Chunk::GlobalLighting)?;
        Ok(times[at])
    }

    /// A light: its ambient color, its diffuse color and its position, each three floats.
    fn light(body: &mut ChunkReader<'_>) -> Result<GameLight, MapError> {
        let mut values = [0.0; 9];
        for value in &mut values {
            *value = body.f32()?;
        }
        if !values.iter().all(|value| value.is_finite()) {
            return Err(MapError::Lighting);
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
    ) -> Result<Vec<MapObject>, MapError> {
        MapFile::version(&chunk, Chunk::ObjectsList, 3..=3)?;
        let mut body = chunk.body;
        let mut objects = Vec::new();
        while !body.is_empty() {
            let mut object = body.chunk()?;
            if *names
                .get(&object.id)
                .ok_or(MapError::UnknownName(object.id))?
                != b"Object"
            {
                continue;
            }
            MapFile::version(&object, Chunk::Object, 3..=3)?;
            let read = MapObject::read(&mut object.body, names)?;
            MapFile::ended(&object.body, Chunk::Object)?;
            if read.kept() {
                objects.push(read);
            }
        }
        Ok(objects)
    }

    fn version(
        chunk: &RawChunk<'_>,
        kind: Chunk,
        versions: RangeInclusive<u16>,
    ) -> Result<(), MapError> {
        if versions.contains(&chunk.version) {
            Ok(())
        } else {
            Err(MapError::Version {
                chunk: kind,
                version: chunk.version,
            })
        }
    }

    fn ended(body: &ChunkReader<'_>, kind: Chunk) -> Result<(), MapError> {
        if body.is_empty() {
            Ok(())
        } else {
            Err(MapError::Leftover(kind))
        }
    }

    fn u32(body: &mut ChunkReader<'_>) -> Result<u32, MapError> {
        let value = body.i32()?;
        u32::try_from(value).ok().ok_or(MapError::Negative(value))
    }

    /// An `i32` index the game keeps in an `i16`.
    fn i16(value: i32) -> Result<i16, MapError> {
        i16::try_from(value).ok().ok_or(MapError::Index(value))
    }

    fn index(value: i16) -> Result<u16, MapError> {
        u16::try_from(value)
            .ok()
            .ok_or(MapError::Index(value.into()))
    }

    /// An index into a list past its unused entry 0: `None` for 0.
    fn entry(value: i16) -> Result<Option<u16>, MapError> {
        Ok(MapFile::index(value)?.checked_sub(1))
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use std::f32::consts::PI;

    use crate::zero_hour::chunk_reader::internals::ChunkWriter;
    use crate::zero_hour::map_object::internals::object;

    /// The heights of the fixture map, 4 × 3 samples in the file's order: the first cell spans
    /// 16 steps, the cells beside it 15, and the cell at column 2, row 1, 34.
    pub(crate) const HEIGHTS: [u8; 12] = [0, 1, 2, 3, 16, 5, 6, 7, 8, 20, 10, 40];

    /// The fixture's `GlobalLighting` of version 3, its time of day afternoon, the second: for
    /// each time `t` from 1, each light `l` of terrain from 0 and of objects from 3 gives the
    /// floats `[t, l, 0, t, l, 1, t, l, 2]`, then the shadows' color.
    pub(crate) fn lighting() -> Vec<u8> {
        let mut bytes = 2_i32.to_le_bytes().to_vec();
        let light = |time: u8, light: u8| {
            [0_u8, 1, 2]
                .into_iter()
                .flat_map(|axis| [f32::from(time), f32::from(light), f32::from(axis)])
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<u8>>()
        };
        for time in 1..=4 {
            for at in [0, 3, 4, 5, 1, 2] {
                bytes.extend(light(time, at));
            }
        }
        bytes.extend(0x0080_8080_u32.to_le_bytes());
        bytes
    }

    /// A map of 4 × 3 samples with a border of 1, its terrain's `BlendTileData` of `version`,
    /// beside a chunk the import skips. Its objects: a tank, a rock 4.5 below the ground at an
    /// angle past π, and a waypoint, which the import keeps; a road's end, and an object too high
    /// for the game to keep, which it does not.
    pub(crate) fn map(version: u16) -> Vec<u8> {
        let mut writer = ChunkWriter::default();
        let heights = [
            &4_i32.to_le_bytes()[..],
            &3_i32.to_le_bytes(),
            &1_i32.to_le_bytes(),
            &1_i32.to_le_bytes(),
            &[2, 0, 0, 0, 1, 0, 0, 0],
            &12_i32.to_le_bytes(),
            &HEIGHTS,
        ]
        .concat();
        let shorts = |values: [i16; 12]| values.map(i16::to_le_bytes).concat();
        let mut terrain = [
            12_i32.to_le_bytes().to_vec(),
            shorts([0, 1, 2, 3, 4, 5, 6, 7, 0, 1, 2, 3]),
            shorts([0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0]),
            shorts([0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0]),
            shorts([0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        ]
        .concat();
        // Version 8 keeps a byte a row: the first sample's bit, then the last sample's of the
        // last row. Version 7 kept `(4 + 1) / 8`, no byte, so no cliff.
        if version == 8 {
            terrain.extend([0b0001, 0b0000, 0b1000]);
        }
        let class = |first: i32, name: &str| {
            [
                &first.to_le_bytes()[..],
                &1_i32.to_le_bytes(),
                &1_i32.to_le_bytes(),
                &0_i32.to_le_bytes(),
                &ChunkWriter::text(name.as_bytes()),
            ]
            .concat()
        };
        let corners = [0.0_f32, 1.0, 0.0, 0.0, 1.5, 0.0, 1.5, 1.0].map(f32::to_le_bytes);
        terrain.extend(
            [
                &2_i32.to_le_bytes()[..],
                &2_i32.to_le_bytes(),
                &2_i32.to_le_bytes(),
                &2_i32.to_le_bytes(),
                &class(0, "Sand"),
                &class(1, "Grass"),
                &0_i32.to_le_bytes(),
                &0_i32.to_le_bytes(),
                &4_i32.to_le_bytes(),
                &[0, 0, 0, 1, 3, 1],
                &(-1_i32).to_le_bytes(),
                &0x7ADA_0000_u32.to_le_bytes(),
                &2_i32.to_le_bytes(),
                &corners.concat(),
                &[1, 0],
            ]
            .concat(),
        );
        let objects = [
            object(
                &mut writer,
                [30.0, 5.0, 0.0],
                PI / 2.0,
                0,
                "AmericaTankCrusader",
                None,
            ),
            object(
                &mut writer,
                [-10.0, 15.0, -4.5],
                4.729_842,
                0,
                "Rocks1",
                None,
            ),
            object(
                &mut writer,
                [12.5, 7.5, 0.0],
                0.0,
                0,
                "*Waypoints/Waypoint",
                Some((3, "Player_1_Start")),
            ),
            object(&mut writer, [0.0, 0.0, 0.0], 0.0, 0x4, "GravelRoad", None),
            object(&mut writer, [0.0, 0.0, 2000.0], 0.0, 0, "Rocks1", None),
        ]
        .map(|body| writer.chunk("Object", 3, &body));
        let chunks = [
            writer.chunk("HeightMapData", 4, &heights),
            writer.chunk("BlendTileData", version, &terrain),
            writer.chunk("WorldInfo", 1, &[0, 0]),
            writer.chunk("ObjectsList", 3, &objects.concat()),
            writer.chunk("GlobalLighting", 3, &lighting()),
        ];
        writer.file(&chunks)
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::PI;
    use std::slice;

    use miniz_oxide::deflate::compress_to_vec_zlib;

    use super::*;
    use crate::zero_hour::chunk_reader::internals::ChunkWriter;
    use crate::zero_hour::map_file::internals::{HEIGHTS, map};
    use crate::zero_hour::map_object::Waypoint;
    use crate::zero_hour::ref_pack::internals::literal;

    #[test]
    fn lighting_of_an_older_version_keeps_the_game_s_dark_lights_and_needs_a_time_of_day() {
        let read = |version: u16, bytes: &[u8]| {
            MapFile::lighting(RawChunk {
                id: 0,
                version,
                body: ChunkReader::new(bytes),
            })
        };
        // Version 1: each time holds its first terrain and object light alone, 18 floats, float
        // `at` of time `t` being `t + at / 32`, exact in an `f32`.
        let mut one = 4_i32.to_le_bytes().to_vec();
        for time in 1..=4_u8 {
            one.extend(
                (0..18_u8).flat_map(|at| (f32::from(time) + f32::from(at) / 32.0).to_le_bytes()),
            );
        }
        let lighting = read(1, &one).unwrap();
        // The night, the fourth: its first terrain light is floats 0 to 8, its first object light
        // 9 to 17; the others as the game starts them.
        assert_eq!(lighting.terrain[0].ambient, [4.0, 4.031_25, 4.0625]);
        assert_eq!(lighting.objects[0].position, [4.468_75, 4.5, 4.531_25]);
        assert_eq!(lighting.terrain[1..], [GameLight::DARK; 2]);
        assert_eq!(lighting.objects[1..], [GameLight::DARK; 2]);
        // A time of day of 0 or 5 is none of the four; a float not finite is refused.
        for time in [0, 5] {
            let mut other = one.clone();
            other[..4].copy_from_slice(&i32::to_le_bytes(time));
            assert_eq!(read(1, &other), Err(MapError::TimeOfDay(time)));
        }
        let mut nan = one.clone();
        nan[4..8].copy_from_slice(&f32::NAN.to_le_bytes());
        assert_eq!(read(1, &nan), Err(MapError::Lighting));
        assert_eq!(
            read(4, &one),
            Err(MapError::Version {
                chunk: Chunk::GlobalLighting,
                version: 4
            })
        );
    }

    #[test]
    fn a_map_reads_its_heights_terrain_and_kept_objects_packed_or_not() {
        let plain = map(8);
        let read = MapFile::read(&plain).unwrap();
        assert_eq!(
            read.heights,
            MapHeights {
                width: 4,
                depth: 3,
                border: 1,
                samples: HEIGHTS.to_vec(),
            }
        );
        // Index 0 names no blend or cliff UVs; 1 names the first.
        let cell = |tile, blend, extra_blend, cliff_uv, cliff| TerrainCell {
            tile,
            blend,
            extra_blend,
            cliff_uv,
            cliff,
        };
        let terrain = &read.terrain;
        assert_eq!(terrain.columns, 4);
        assert_eq!(terrain.cells[0], cell(0, None, None, None, true));
        assert_eq!(terrain.cells[2], cell(2, None, None, Some(0), false));
        assert_eq!(terrain.cells[5], cell(5, Some(0), None, None, false));
        assert_eq!(terrain.cells[6], cell(6, None, Some(0), None, false));
        assert_eq!(terrain.cells[11], cell(3, None, None, None, true));
        assert_eq!(terrain.cells.iter().filter(|cell| cell.cliff).count(), 2);
        assert_eq!(terrain.tiles, 2);
        assert_eq!(
            terrain
                .classes
                .iter()
                .map(|class| &class.name[..])
                .collect::<Vec<_>>(),
            ["Sand", "Grass"]
        );
        assert_eq!(
            terrain.blends,
            [BlendTile {
                tile: 4,
                shape: BlendShape::LeftDiagonal { long: true },
                inverted: true,
                flipped: true,
                custom_edge: None,
            }]
        );
        assert_eq!(
            terrain.cliffs,
            [CliffUv {
                tile: 2,
                corners: [[0.0, 1.0], [0.0, 0.0], [1.5, 0.0], [1.5, 1.0]],
                flip: true,
                mutant: false,
            }]
        );
        // The road's end is read and kept; the object at 2,000 is not.
        let templates: Vec<&[u8]> = read
            .objects
            .iter()
            .map(|object| &object.template[..])
            .collect();
        assert_eq!(
            templates,
            [
                &b"AmericaTankCrusader"[..],
                b"Rocks1",
                b"*Waypoints/Waypoint",
                b"GravelRoad"
            ]
        );
        assert_eq!(read.objects[0].pos, [30.0, 5.0, 0.0]);
        assert_eq!(read.objects[0].angle, PI / 2.0);
        assert_eq!(
            read.objects[2].waypoint,
            Some(Waypoint {
                id: 3,
                name: b"Player_1_Start".to_vec()
            })
        );
        assert_eq!(read.objects[0].waypoint, None);
        assert!(read.objects[3].road_or_bridge());
        // The lights of its time of day, the afternoon, the second: terrain lights 0 to 2 and
        // object lights 3 to 5 of time 2.
        let light = |at: u8| {
            let [time, at] = [2.0, f32::from(at)];
            GameLight {
                ambient: [time, at, 0.0],
                diffuse: [time, at, 1.0],
                position: [time, at, 2.0],
            }
        };
        assert_eq!(
            read.lighting,
            MapLighting {
                terrain: [0, 1, 2].map(light),
                objects: [3, 4, 5].map(light),
            }
        );
    }

    #[test]
    fn a_packed_map_reads_as_its_plain_file() {
        // Packed by RefPack and by zlib, each behind its 8-byte header, it reads the same.
        let plain = map(8);
        let read = MapFile::read(&plain).unwrap();
        let len = u32::try_from(plain.len()).unwrap().to_le_bytes();
        let ear = [&b"EAR\0"[..], &len, &literal(&plain)].concat();
        let zlib = [&b"ZL5\0"[..], &len, &compress_to_vec_zlib(&plain, 5)].concat();
        assert_eq!(MapFile::read(&ear).unwrap(), read);
        assert_eq!(MapFile::read(&zlib).unwrap(), read);
        let short = [
            &b"ZL5\0"[..],
            &(u32::from_le_bytes(len) - 1).to_le_bytes(),
            &compress_to_vec_zlib(&plain, 5),
        ]
        .concat();
        assert!(matches!(MapFile::read(&short), Err(MapError::Zlib(_))));
        let long = [
            &b"EAR\0"[..],
            &(u32::from_le_bytes(len) + 1).to_le_bytes(),
            &literal(&plain),
        ]
        .concat();
        assert_eq!(
            MapFile::read(&long),
            Err(MapError::PackedSize {
                written: plain.len(),
                len: plain.len() + 1
            })
        );
        let nox = [&b"NOX\0"[..], &len, &plain].concat();
        assert_eq!(MapFile::read(&nox), Err(MapError::Packing(Packing::Nox)));
    }

    #[test]
    fn a_map_of_an_older_terrain_takes_its_cliffs_as_the_game_does() {
        // Version 7 stored no byte for a row of 4, so no cell is a cliff; version 6 stored none,
        // so the game marks the cells whose corners span 16 steps or more: the first, of 0, 1,
        // 16 and 5, and the one at column 2, row 1, of 6, 7, 10 and 40; not the two of 15.
        let cliffs = |version| -> Vec<usize> {
            let terrain = MapFile::read(&map(version)).unwrap().terrain;
            (0..12).filter(|&at| terrain.cells[at].cliff).collect()
        };
        assert_eq!(cliffs(7), Vec::<usize>::new());
        assert_eq!(cliffs(6), [0, 6]);
        assert_eq!(cliffs(8), [0, 11]);
        assert_eq!(
            MapFile::read(&map(5)),
            Err(MapError::Version {
                chunk: Chunk::BlendTileData,
                version: 5
            })
        );
    }

    #[test]
    fn a_map_the_game_would_misread_is_refused() {
        let mut writer = ChunkWriter::default();
        let heights = |len: i32| {
            [
                &2_i32.to_le_bytes()[..],
                &2_i32.to_le_bytes(),
                &0_i32.to_le_bytes(),
                &0_i32.to_le_bytes(),
                &len.to_le_bytes(),
                &[0; 4],
            ]
            .concat()
        };
        let good = writer.chunk("HeightMapData", 4, &heights(4));
        let odd = writer.chunk("HeightMapData", 4, &heights(3));
        let file = |writer: &ChunkWriter, chunks: &[Vec<u8>]| MapFile::read(&writer.file(chunks));
        assert_eq!(
            file(&writer, slice::from_ref(&good)),
            Err(MapError::Missing(Chunk::BlendTileData))
        );
        assert_eq!(
            file(&writer, &[good.clone(), good.clone()]),
            Err(MapError::Twice(Chunk::HeightMapData))
        );
        let blend = writer.chunk("BlendTileData", 8, &[]);
        assert_eq!(
            file(&writer, &[odd, blend]),
            Err(MapError::Samples {
                len: 3,
                width: 2,
                depth: 2
            })
        );
        assert_eq!(MapFile::read(b"CkMq"), Err(MapError::NotChunks));
        // A chunk of an id the table lacks; the fixture cut by a byte.
        let stray = [&99_u32.to_le_bytes()[..], &[1, 0], &0_i32.to_le_bytes()].concat();
        assert_eq!(file(&writer, &[stray]), Err(MapError::UnknownName(99)));
        let cut = map(8);
        assert_eq!(MapFile::read(&cut[..cut.len() - 1]), Err(MapError::Short));
    }

    #[test]
    fn a_blend_tile_sets_one_direction_and_known_bits() {
        let blend = |flags: [u8; 6], mark: u32| {
            let bytes = [
                &4_i32.to_le_bytes()[..],
                &flags,
                &(-1_i32).to_le_bytes(),
                &mark.to_le_bytes(),
            ]
            .concat();
            MapFile::blend_tile(&mut ChunkReader::new(&bytes), 1)
        };
        assert_eq!(
            blend([1, 0, 0, 0, 2, 1], BLEND_MARK).map(|tile| (
                tile.shape,
                tile.inverted,
                tile.flipped
            )),
            Ok((BlendShape::Horizontal, false, true))
        );
        assert_eq!(
            blend([0, 0, 1, 0, 0, 0], BLEND_MARK).map(|tile| tile.shape),
            Ok(BlendShape::RightDiagonal { long: false })
        );
        assert_eq!(
            blend([0, 0, 0, 0, 0, 0], BLEND_MARK),
            Err(MapError::BlendShape(1))
        );
        assert_eq!(
            blend([1, 1, 0, 0, 0, 0], BLEND_MARK),
            Err(MapError::BlendShape(1))
        );
        assert_eq!(
            blend([0, 1, 0, 0, 4, 0], BLEND_MARK),
            Err(MapError::BlendInverted(1))
        );
        assert_eq!(blend([0, 1, 0, 0, 0, 0], 0), Err(MapError::BlendMark(1)));
    }
}
