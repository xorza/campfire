use campfire_math::Num;
use campfire_package::{
    BlendShape, BlendTile, ClassTexture, GameData, TerrainCell, TerrainParts, TextureClass,
};

use super::*;

/// 5 × 4 samples, 4 × 3 cells, 10 m apart, a step of 0.625 m, the first sample, the game's
/// north-west, at `x = −10`, `z = −20`: so the game's `[x, y]` lies at `x = −10 + 10x`,
/// `z = −20 + 10 · (3 − y)`. The game's samples `[0, 0]` and `[2, 1]` stand 16 steps, 10 m, up.
/// Cell `[1, 0]` blends along a short right diagonal, which flips it; cell `[3, 2]` blends
/// across, with a third blend over it.
fn fixture() -> (TerrainParts, HeightGrid, Vec<u8>) {
    let at = |[x, y]: [usize; 2]| (3 - y) * 5 + x;
    let mut samples = vec![0; 20];
    samples[at([0, 0])] = 16;
    samples[at([2, 1])] = 16;
    let grid = HeightGrid::new(
        [Num::from_int(-10).unwrap(), Num::from_int(-20).unwrap()],
        Num::from_int(10).unwrap(),
        Num::from_bits(10 << (Num::FRAC_BITS - 4)),
        5,
        samples,
    )
    .unwrap();
    let blend = |shape| BlendTile {
        tile: 0,
        shape,
        inverted: false,
        flipped: false,
        custom_edge: None,
    };
    let mut cells = vec![
        TerrainCell {
            tile: 8,
            blend: None,
            extra_blend: None,
            cliff_uv: None,
            cliff: false,
        };
        20
    ];
    cells[at([1, 0])].blend = Some(0);
    cells[at([3, 2])].blend = Some(1);
    cells[at([3, 2])].extra_blend = Some(1);
    let parts = TerrainParts {
        columns: 5,
        cells,
        tiles: 4,
        classes: vec![TextureClass {
            first_tile: 0,
            tiles: 4,
            width: 2,
            name: "Sand".to_owned(),
            texture: None,
        }],
        edge_tiles: 0,
        edge_classes: Vec::new(),
        blends: vec![
            blend(BlendShape::RightDiagonal { long: false }),
            blend(BlendShape::Horizontal),
        ],
        cliffs: Vec::new(),
    };
    let texture = (0..3)
        .flat_map(|level| vec![9_u8; ((128 >> level) * (128 >> level) * 4) as usize])
        .collect();
    (parts, grid, texture)
}

fn meshes(game: GameData) -> TerrainMeshes {
    let (parts, grid, texture) = fixture();
    let textures = [Some(ClassTexture {
        width: 128,
        height: 128,
        data: texture.clone(),
    })];
    let atlas = TerrainAtlas::new(&parts, &textures);
    let cells = TerrainCells::new(&parts, grid.samples(), &atlas, game);
    TerrainMeshes::of(&cells, &grid, &atlas)
}

const THREE_WAY: GameData = GameData {
    adjust_cliff_textures: false,
    three_way_blends: true,
};

#[test]
fn each_cell_lies_on_its_samples_and_splits_as_its_blend_says() {
    let TerrainMeshes { ground, overlay } = meshes(THREE_WAY);
    // 4 × 3 cells, four corners and two triangles each, row by row from the game's south.
    assert_eq!((ground.positions.len(), ground.indices.len()), (48, 72));
    // Cell [0, 0]: corners [0, 0], [1, 0], [1, 1] and [0, 1], the first 10 m up.
    assert_eq!(
        ground.positions[..4],
        [
            [-10.0, 10.0, 10.0],
            [0.0, 0.0, 10.0],
            [0.0, 0.0, 0.0],
            [-10.0, 0.0, 0.0]
        ]
    );
    // Cell [2, 1], the sixth after [0, 0]: its first corner, [2, 1], 10 m up.
    assert_eq!(ground.positions[24], [10.0, 10.0, 0.0]);
    // Unflipped, a cell meets on its diagonal from corner 0 to 2; cell [1, 0], flipped by its
    // short right diagonal, on 1 to 3.
    assert_eq!(ground.indices[..6], [0, 1, 2, 0, 2, 3]);
    assert_eq!(ground.indices[6..12], [5, 6, 7, 5, 7, 4]);
    // Its tile, source tile 2's south-west quarter, from [4, 36] to [36, 68] of the atlas,
    // 2,048 by 256: its block and border reach 136 texels down.
    assert_eq!(
        ground.uvs[..4],
        [
            [4.0 / 2048.0, 68.0 / 256.0],
            [36.0 / 2048.0, 68.0 / 256.0],
            [36.0 / 2048.0, 36.0 / 256.0],
            [4.0 / 2048.0, 36.0 / 256.0],
        ]
    );
    // The overlay: cell [1, 0]'s blend, opaque at its north-east corner and flipped; then cell
    // [3, 2]'s blend and its third blend, each opaque at its east corners, unflipped.
    assert_eq!(overlay.positions.len(), 12);
    let alpha: Vec<f32> = overlay.colors.iter().map(|color| color[3]).collect();
    assert_eq!(
        alpha,
        [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0]
    );
    assert_eq!(
        overlay.indices,
        [1, 2, 3, 1, 3, 0, 4, 5, 6, 4, 6, 7, 8, 9, 10, 8, 10, 11]
    );
    // Cell [3, 2]'s first corner, [3, 2], at x = 20, z = −10, flat.
    assert_eq!(overlay.positions[4], [20.0, 0.0, -10.0]);
    // Without third blends, cell [3, 2] has its blend alone.
    let TerrainMeshes { overlay, .. } = meshes(GameData {
        three_way_blends: false,
        ..THREE_WAY
    });
    assert_eq!(overlay.positions.len(), 8);
}

#[test]
fn a_sample_s_normal_crosses_its_slopes_across_its_neighbors() {
    let TerrainMeshes { ground, .. } = meshes(THREE_WAY);
    // Sample [1, 1], corner 2 of cell [0, 0]: 10 m rise east, from [0, 1] to [2, 1], and none
    // north, from [1, 0] to [1, 2]: (20, 0, 10) × (0, 20, 0) = (−200, 0, 400) on the game's
    // axes, (−200, 400, 0) on the engine's, over √200000.
    let [x, y, z] = ground.normals[2];
    let length = 200_000_f32.sqrt();
    assert_eq!([x, y, z], [-200.0 / length, 400.0 / length, 0.0]);
    // Sample [0, 0], corner 0 of cell [0, 0], at the map's corner: its neighbors west and south
    // clamp to itself, still 20 m apart, so it falls 10 m east and 10 m north: (200, 400, −200)
    // on the engine's axes, over √240000.
    let length = 240_000_f32.sqrt();
    assert_eq!(
        ground.normals[0],
        [200.0 / length, 400.0 / length, -200.0 / length]
    );
    // Sample [2, 2], corner 2 of cell [1, 1]: it falls 10 m north, from [2, 1] to [2, 3]; its
    // normal leans north, the engine's −z.
    let length = 200_000_f32.sqrt();
    assert_eq!(ground.normals[22], [0.0, 400.0 / length, -200.0 / length]);
}
