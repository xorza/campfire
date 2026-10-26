use std::f32::consts::PI;
use std::slice;

use miniz_oxide::deflate::compress_to_vec_zlib;

use super::*;
use crate::zero_hour::format::map::chunk_reader::samples::ChunkWriter;
use crate::zero_hour::format::map::map_file::samples::{HEIGHTS, map};
use crate::zero_hour::format::map::map_object::Waypoint;
use crate::zero_hour::format::ref_pack::samples::literal;

#[test]
fn lighting_of_an_older_version_keeps_the_game_s_dark_lights_and_needs_a_time_of_day() {
    let read = |version: u16, bytes: &[u8]| {
        ZeroHourMap::lighting(RawChunk {
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
    let lights = lighting.lights;
    // The night, the fourth: its first terrain light is floats 0 to 8, which the import drops,
    // its first object light 9 to 17; the others as the game starts them. Its models follow
    // the night; at the evening, the third, they do not.
    assert!(lighting.night);
    let mut evening = one.clone();
    evening[..4].copy_from_slice(&3_i32.to_le_bytes());
    assert!(!read(1, &evening).unwrap().night);
    assert_eq!(lights[0].ambient, [4.28125, 4.3125, 4.34375]);
    assert_eq!(lights[0].position, [4.468_75, 4.5, 4.531_25]);
    assert_eq!(lights[1..], [GameLight::DARK; 2]);
    // A time of day of 0 or 5 is none of the four; a float not finite is refused.
    for time in [0, 5] {
        let mut other = one.clone();
        other[..4].copy_from_slice(&i32::to_le_bytes(time));
        assert_eq!(read(1, &other), Err(MapReadError::TimeOfDay(time)));
    }
    let mut nan = one.clone();
    nan[4..8].copy_from_slice(&f32::NAN.to_le_bytes());
    assert_eq!(read(1, &nan), Err(MapReadError::Lighting));
    assert_eq!(
        read(4, &one),
        Err(MapReadError::Version {
            chunk: Chunk::GlobalLighting,
            version: 4
        })
    );
}

#[test]
fn a_world_snows_at_its_snowy_weather_alone() {
    // A `weather` of 1, `WEATHER_SNOWY`, snows; of 0 or 2, of text, or none, it does not; a
    // key set twice takes its last value; a dictionary of another version than 1 is refused.
    let snowy_of = |version: u16, entries: &[(&str, u8, Vec<u8>)]| {
        let mut writer = ChunkWriter::default();
        let count = u16::try_from(entries.len()).unwrap();
        let mut body = count.to_le_bytes().to_vec();
        for (key, kind, value) in entries {
            body.extend(writer.entry(key, *kind, value));
        }
        let chunk = RawChunk {
            id: 0,
            version,
            body: ChunkReader::new(&body),
        };
        ZeroHourMap::snowy(chunk, &writer.table())
    };
    let snowy = |entries: &[(&str, u8, Vec<u8>)]| snowy_of(1, entries);
    let int = |value: i32| value.to_le_bytes().to_vec();
    for version in [0, 2] {
        assert_eq!(
            snowy_of(version, &[]),
            Err(MapReadError::Version {
                chunk: Chunk::WorldInfo,
                version
            })
        );
    }
    assert_eq!(snowy(&[("weather", 1, int(1))]), Ok(true));
    for weather in [0, 2] {
        assert_eq!(snowy(&[("weather", 1, int(weather))]), Ok(false));
    }
    let text = ChunkWriter::text(b"snowy");
    assert_eq!(snowy(&[("weather", 3, text)]), Ok(false));
    assert_eq!(snowy(&[("mapName", 1, int(1))]), Ok(false));
    assert_eq!(snowy(&[]), Ok(false));
    let twice = [("weather", 1, int(0)), ("weather", 1, int(1))];
    assert_eq!(snowy(&twice), Ok(true));
    // The fixture's world dictionary is empty: a day map, not snowy.
    let read = ZeroHourMap::read(&map(8)).unwrap();
    assert_eq!(read.conditions, LookConditions::default());
}

#[test]
fn a_map_reads_its_heights_terrain_and_kept_objects_packed_or_not() {
    let plain = map(8);
    let read = ZeroHourMap::read(&plain).unwrap();
    assert_eq!(
        read.heights,
        MapHeights {
            width: 4,
            depth: 3,
            border: 1,
            boundary: Some([2, 1]),
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
    assert_eq!(terrain.fields().cells[0], cell(0, None, None, None, true));
    assert_eq!(
        terrain.fields().cells[2],
        cell(2, None, None, Some(0), false)
    );
    assert_eq!(
        terrain.fields().cells[5],
        cell(5, Some(0), None, None, false)
    );
    assert_eq!(
        terrain.fields().cells[6],
        cell(6, None, Some(0), None, false)
    );
    assert_eq!(terrain.fields().cells[11], cell(3, None, None, None, true));
    assert_eq!(
        terrain
            .fields()
            .cells
            .iter()
            .filter(|cell| cell.cliff)
            .count(),
        2
    );
    assert_eq!(terrain.fields().tiles, 2);
    assert_eq!(
        terrain
            .fields()
            .classes
            .iter()
            .map(|class| class.name.as_str())
            .collect::<Vec<_>>(),
        ["Sand", "Grass"]
    );
    assert_eq!(
        terrain.fields().blends,
        [BlendTile {
            tile: 4,
            shape: BlendShape::LeftDiagonal { long: true },
            inverted: true,
            flipped: true,
            custom_edge: None,
        }]
    );
    assert_eq!(
        terrain.fields().cliffs,
        [CliffUv {
            tile: 2,
            corners: [[0.0, 1.0], [0.0, 0.0], [1.5, 0.0], [1.5, 1.0]],
            flip: true,
            mutant: false,
        }]
    );
    // The road's end is read and kept, as is every template; the object at 2,000 is not.
    let templates: Vec<&[u8]> = read
        .objects
        .iter()
        .map(|object| object.template.as_slice())
        .collect();
    assert_eq!(
        templates,
        [
            &b"AmericaTankCrusader"[..],
            b"Rocks1",
            b"*Waypoints/Waypoint",
            b"GravelRoad",
            b"GravelRoad",
            b"Oak",
            b"Rock",
            b"RockRed",
            b"Bush",
            b"FortressWall",
            b"Ghost",
            b"Nowhere",
            b"rock",
        ]
    );
    assert_eq!(read.objects[0].position, [30.0, 5.0, 0.0]);
    assert_eq!(read.objects[0].angle, PI / 2.0);
    assert_eq!(
        read.objects[2].waypoint,
        Some(Waypoint {
            id: 3,
            name: b"Fixture_Center".to_vec()
        })
    );
    assert_eq!(read.objects[0].waypoint, None);
    assert!(read.objects[3].is_road_start() && read.objects[4].is_road_end());
    assert!(read.objects[3].is_road_or_bridge() && !read.objects[5].is_road_or_bridge());
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
    assert_eq!(read.lights, [3, 4, 5].map(light));
}

#[test]
fn a_packed_map_reads_as_its_plain_file() {
    // Packed by RefPack and by zlib, each behind its 8-byte header, it reads the same.
    let plain = map(8);
    let read = ZeroHourMap::read(&plain).unwrap();
    let len = u32::try_from(plain.len()).unwrap().to_le_bytes();
    let ear = [&b"EAR\0"[..], &len, &literal(&plain)].concat();
    let zlib = [&b"ZL5\0"[..], &len, &compress_to_vec_zlib(&plain, 5)].concat();
    assert_eq!(ZeroHourMap::read(&ear).unwrap(), read);
    assert_eq!(ZeroHourMap::read(&zlib).unwrap(), read);
    let short = [
        &b"ZL5\0"[..],
        &(u32::from_le_bytes(len) - 1).to_le_bytes(),
        &compress_to_vec_zlib(&plain, 5),
    ]
    .concat();
    assert!(matches!(
        ZeroHourMap::read(&short),
        Err(MapReadError::Zlib(_))
    ));
    let long = [
        &b"EAR\0"[..],
        &(u32::from_le_bytes(len) + 1).to_le_bytes(),
        &literal(&plain),
    ]
    .concat();
    assert_eq!(
        ZeroHourMap::read(&long),
        Err(MapReadError::PackedSize {
            written: plain.len(),
            len: plain.len() + 1
        })
    );
    let nox = [&b"NOX\0"[..], &len, &plain].concat();
    assert_eq!(
        ZeroHourMap::read(&nox),
        Err(MapReadError::Packing(Packing::Nox))
    );
}

#[test]
fn a_map_of_an_older_terrain_takes_its_cliffs_as_the_game_does() {
    // Version 7 stored no byte for a row of 4, so no cell is a cliff; version 6 stored none,
    // so the game marks the cells whose corners span 16 steps or more: the first, of 0, 1,
    // 16 and 5, and the one at column 2, row 1, of 6, 7, 10 and 40; not the two of 15.
    let cliffs = |version| -> Vec<usize> {
        let terrain = ZeroHourMap::read(&map(version)).unwrap().terrain;
        (0..12)
            .filter(|&at| terrain.fields().cells[at].cliff)
            .collect()
    };
    assert_eq!(cliffs(7), Vec::<usize>::new());
    assert_eq!(cliffs(6), [0, 6]);
    assert_eq!(cliffs(8), [0, 11]);
    assert_eq!(
        ZeroHourMap::read(&map(5)),
        Err(MapReadError::Version {
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
    let file = |writer: &ChunkWriter, chunks: &[Vec<u8>]| ZeroHourMap::read(&writer.file(chunks));
    assert_eq!(
        file(&writer, slice::from_ref(&good)),
        Err(MapReadError::Missing(Chunk::BlendTileData))
    );
    assert_eq!(
        file(&writer, &[good.clone(), good]),
        Err(MapReadError::Twice(Chunk::HeightMapData))
    );
    let blend = writer.chunk("BlendTileData", 8, &[]);
    assert_eq!(
        file(&writer, &[odd, blend]),
        Err(MapReadError::Samples {
            len: 3,
            width: 2,
            depth: 2
        })
    );
    assert_eq!(ZeroHourMap::read(b"CkMq"), Err(MapReadError::NotChunks));
    // A chunk of an id the table lacks; the fixture cut by a byte.
    let stray = [&99_u32.to_le_bytes()[..], &[1, 0], &0_i32.to_le_bytes()].concat();
    assert_eq!(file(&writer, &[stray]), Err(MapReadError::UnknownName(99)));
    let cut = map(8);
    assert_eq!(
        ZeroHourMap::read(&cut[..cut.len() - 1]),
        Err(MapReadError::Short)
    );
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
        ZeroHourMap::blend_tile(&mut ChunkReader::new(&bytes), 1)
    };
    assert_eq!(
        blend([1, 0, 0, 0, 2, 1], BLEND_MARK).map(|tile| (tile.shape, tile.inverted, tile.flipped)),
        Ok((BlendShape::Horizontal, false, true))
    );
    assert_eq!(
        blend([0, 0, 1, 0, 0, 0], BLEND_MARK).map(|tile| tile.shape),
        Ok(BlendShape::RightDiagonal { long: false })
    );
    assert_eq!(
        blend([0, 0, 0, 0, 0, 0], BLEND_MARK),
        Err(MapReadError::BlendShape(1))
    );
    assert_eq!(
        blend([1, 1, 0, 0, 0, 0], BLEND_MARK),
        Err(MapReadError::BlendShape(1))
    );
    assert_eq!(
        blend([0, 1, 0, 0, 4, 0], BLEND_MARK),
        Err(MapReadError::BlendInverted(1))
    );
    assert_eq!(
        blend([0, 1, 0, 0, 0, 0], 0),
        Err(MapReadError::BlendMark(1))
    );
}
