use std::f32::consts::PI;

use crate::zero_hour::format::map::chunk_reader::samples::ChunkWriter;
use crate::zero_hour::format::map::map_object::samples::object;

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
/// angle past π, and a waypoint, which the import keeps; a road's start and end; a tree past the
/// heights, a prop 1 above the ground, its reskin, fluff, a wall a unit walks on, a prop of no
/// model, a template no object names and one of another case; and an object too high for the
/// game to keep, which it does not.
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
            Some((3, "Fixture_Center")),
        ),
        object(&mut writer, [-10.0, 5.0, 0.0], 0.0, 0x2, "GravelRoad", None),
        object(&mut writer, [20.0, 5.0, 0.0], 0.0, 0x4, "GravelRoad", None),
        object(&mut writer, [50.0, 0.0, 0.0], 0.0, 0, "Oak", None),
        object(&mut writer, [5.0, 5.0, 1.0], 0.0, 0, "Rock", None),
        object(&mut writer, [5.0, -5.0, 0.0], 0.0, 0, "RockRed", None),
        object(&mut writer, [0.0, 5.0, 0.0], 0.0, 0, "Bush", None),
        object(&mut writer, [10.0, 0.0, 0.0], 0.0, 0, "FortressWall", None),
        object(&mut writer, [0.0, 0.0, 0.0], 0.0, 0, "Ghost", None),
        object(&mut writer, [0.0, 0.0, 0.0], 0.0, 0, "Nowhere", None),
        object(&mut writer, [0.0, 0.0, 0.0], 0.0, 0, "rock", None),
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
