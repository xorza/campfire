use campfire_package::{ClassTexture, CliffUv, TextureClass};

use super::*;

const COLUMNS: usize = 5;
const COLUMNS_U32: u32 = 5;
const ROWS: usize = 4;

/// 5 × 4 samples, 4 × 3 cells, all flat at 0: class 0, `Sand`, of 2 × 2 tiles with a texture,
/// its block at the atlas's `[4, 4]`, so source tile 0 lies at `[4, 68]`, 1 at `[68, 68]`, 2 at
/// `[4, 4]` and 3 at `[68, 4]`; class 1, `Bare`, of one tile, with none. Every cell draws tile 8,
/// source tile 2's south-west quarter.
struct Fixture {
    parts: TerrainParts,
    samples: Vec<u8>,
    texture: Vec<u8>,
}

impl Fixture {
    fn new() -> Fixture {
        let class = |first_tile, tiles, width, name: &str| TextureClass {
            first_tile,
            tiles,
            width,
            name: name.to_owned(),
            texture: None,
        };
        let cell = TerrainCell {
            tile: 8,
            blend: None,
            extra_blend: None,
            cliff_uv: None,
            cliff: false,
        };
        let texture = (0..3)
            .flat_map(|level| vec![9_u8; ((128 >> level) * (128 >> level) * 4) as usize])
            .collect();
        Fixture {
            parts: TerrainParts {
                columns: COLUMNS_U32,
                cells: vec![cell; COLUMNS * ROWS],
                tiles: 5,
                classes: vec![class(0, 4, 2, "Sand"), class(4, 1, 1, "Bare")],
                edge_tiles: 0,
                edge_classes: Vec::new(),
                blends: Vec::new(),
                cliffs: Vec::new(),
            },
            samples: vec![0; COLUMNS * ROWS],
            texture,
        }
    }

    /// The package's index of the game's `[x, y]`: its rows run from the game's north.
    const fn at([x, y]: [usize; 2]) -> usize {
        (ROWS - 1 - y) * COLUMNS + x
    }

    fn cell(&mut self, at: [usize; 2]) -> &mut TerrainCell {
        &mut self.parts.cells[Fixture::at(at)]
    }

    /// Sets the heights of the cell `[x, y]`'s corners, in the game's order.
    fn corners(&mut self, [x, y]: [usize; 2], heights: [u8; 4]) {
        for (corner, height) in [[x, y], [x + 1, y], [x + 1, y + 1], [x, y + 1]]
            .into_iter()
            .zip(heights)
        {
            self.samples[Fixture::at(corner)] = height;
        }
    }

    fn atlas(&self) -> TerrainAtlas {
        let textures = [
            Some(ClassTexture {
                width: 128,
                height: 128,
                data: self.texture.clone(),
            }),
            None,
        ];
        TerrainAtlas::new(&self.parts, &textures)
    }

    /// What the cell `at` draws, with `game`'s settings.
    fn draw(&self, at: [usize; 2], game: GameData) -> CellDraw {
        let atlas = self.atlas();
        TerrainCells::new(&self.parts, &self.samples, &atlas, game).cell(at)
    }
}

const ADJUSTED: GameData = GameData {
    adjust_cliff_textures: true,
    three_way_blends: true,
};

const PLAIN: GameData = GameData {
    adjust_cliff_textures: false,
    three_way_blends: false,
};

/// A blend of `tile` in `shape`.
const fn blend(tile: u16, shape: BlendShape, inverted: bool, flipped: bool) -> BlendTile {
    BlendTile {
        tile,
        shape,
        inverted,
        flipped,
        custom_edge: None,
    }
}

/// A layer's alpha at its cell's centre, which lies on the diagonal its triangles meet on.
fn centre(layer: Layer) -> f32 {
    let [a0, a1, a2, a3] = layer.alpha.map(f32::from);
    if layer.flip {
        f32::midpoint(a1, a3)
    } else {
        f32::midpoint(a0, a2)
    }
}

#[test]
fn a_tile_draws_its_quarter_of_its_source_tile_and_one_with_no_texture_none() {
    let mut fixture = Fixture::new();
    // Tile 8 is source tile 2, at [4, 4], quarter 0: its west half and its south half, `v` from
    // 4 + 32 = 36 down to 68, the south-west corner at the bottom left.
    let draw = fixture.draw([0, 0], PLAIN);
    assert_eq!(
        draw.tile,
        [[4.0, 68.0], [36.0, 68.0], [36.0, 36.0], [4.0, 36.0]]
    );
    // With no blend, the blend layer is the tile, clear, unflipped.
    assert_eq!(
        draw.blend,
        Layer {
            uv: draw.tile,
            alpha: [0; 4],
            flip: false,
        }
    );
    // Tile 7 is source tile 1, at [68, 68], quarter 3: its east half from 100 and its north
    // half from 68.
    fixture.cell([1, 0]).tile = 7;
    assert_eq!(
        fixture.draw([1, 0], PLAIN).tile,
        [[100.0, 100.0], [132.0, 100.0], [132.0, 68.0], [100.0, 68.0]]
    );
    // Tile 16 is source tile 4, of `Bare`, which has no texture.
    fixture.cell([2, 0]).tile = 16;
    assert_eq!(fixture.draw([2, 0], ADJUSTED).tile, [[0.0; 2]; 4]);
}

#[test]
fn each_blend_is_opaque_at_its_corners_and_splits_its_cell_along_its_fade() {
    let fixture = Fixture::new();
    let atlas = fixture.atlas();
    let cells = TerrainCells::new(&fixture.parts, &fixture.samples, &atlas, ADJUSTED);
    let plain = TerrainCells::new(&fixture.parts, &fixture.samples, &atlas, PLAIN);
    let horizontal = BlendShape::Horizontal;
    let vertical = BlendShape::Vertical;
    let right = |long| BlendShape::RightDiagonal { long };
    let left = |long| BlendShape::LeftDiagonal { long };
    // Each shape, inverted or not: its alpha at the corners south-west, south-east, north-east,
    // north-west, its flip, and its alpha at the centre, the mean of the split's two ends.
    let cases = [
        (horizontal, false, [0, 255, 255, 0], false, 127.5),
        (horizontal, true, [255, 0, 0, 255], false, 127.5),
        (vertical, false, [0, 0, 255, 255], false, 127.5),
        (vertical, true, [255, 255, 0, 0], false, 127.5),
        (right(false), false, [0, 0, 255, 0], true, 0.0),
        (right(false), true, [0, 255, 0, 0], false, 0.0),
        (right(true), false, [0, 255, 255, 255], true, 255.0),
        (right(true), true, [255, 255, 255, 0], false, 255.0),
        (left(false), false, [0, 0, 0, 255], false, 0.0),
        (left(false), true, [255, 0, 0, 0], true, 0.0),
        (left(true), false, [255, 0, 255, 255], false, 255.0),
        (left(true), true, [255, 255, 0, 255], true, 255.0),
    ];
    for (shape, inverted, alpha, flip, middle) in cases {
        let pattern = cells.pattern(&blend(0, shape, inverted, false));
        assert_eq!(
            (pattern.alpha, pattern.flip),
            (alpha, flip),
            "{shape:?} {inverted}"
        );
        let layer = Layer {
            uv: [[0.0; 2]; 4],
            alpha: pattern.alpha,
            flip: pattern.flip,
        };
        assert_eq!(centre(layer), middle, "{shape:?} {inverted}");
    }
    // A third blend's flip turns a blend across or along the cell, unless third blends are off;
    // a custom edge has no alpha and no flip.
    let flipped = blend(0, horizontal, false, true);
    assert!(cells.pattern(&flipped).flip);
    assert!(!plain.pattern(&flipped).flip);
    let edge = BlendTile {
        custom_edge: Some(0),
        ..blend(0, right(false), false, false)
    };
    let pattern = cells.pattern(&edge);
    assert_eq!((pattern.alpha, pattern.flip), ([0; 4], false));
}

#[test]
fn a_cell_draws_its_blend_and_its_third_blend_when_the_game_draws_them() {
    let mut fixture = Fixture::new();
    fixture.parts.blends = vec![
        blend(4, BlendShape::RightDiagonal { long: false }, false, false),
        blend(0, BlendShape::Vertical, true, true),
    ];
    let cell = fixture.cell([1, 1]);
    cell.blend = Some(0);
    cell.extra_blend = Some(1);
    // The blend is tile 4, source tile 1's south-west quarter, at [68, 100] to [100, 132].
    let draw = fixture.draw([1, 1], ADJUSTED);
    assert_eq!(
        draw.blend,
        Layer {
            uv: [[68.0, 132.0], [100.0, 132.0], [100.0, 100.0], [68.0, 100.0]],
            alpha: [0, 0, 255, 0],
            flip: true,
        }
    );
    // The third blend, tile 0, source tile 0's south-west quarter, inverted along the cell, and
    // flipped as it asks.
    assert_eq!(
        draw.extra,
        Some(Layer {
            uv: [[4.0, 132.0], [36.0, 132.0], [36.0, 100.0], [4.0, 100.0]],
            alpha: [255, 255, 0, 0],
            flip: true,
        })
    );
    assert_eq!(fixture.draw([1, 1], PLAIN).extra, None);
}

#[test]
fn a_steep_cell_stretches_its_tile_by_its_heights_or_its_cliff_uvs() {
    let mut fixture = Fixture::new();
    // The north-west corner 40 steps up: a rise of 40 is a slope of 40 · 0.0625 = 2.5. Below
    // 0 + (80 + 1) / 3 = 27 lie three corners, above 0 + 41 / 3 = 13 one: a lone high corner,
    // whose `v` moves a share 1 / divisor of the 128-texel block, the divisor 4 / 2.5 = 1.6: by
    // 80, from 68 to −12. Then the tile moves down by 4 − (−12) = 16, back inside the block.
    fixture.corners([2, 1], [0, 0, 0, 40]);
    let draw = fixture.draw([2, 1], ADJUSTED);
    assert_eq!(
        draw.tile,
        [[4.0, 84.0], [36.0, 84.0], [36.0, 52.0], [4.0, 4.0]]
    );
    // Stretched, its split climbs least: |0 − 0| is not past |0 − 40|, so no flip.
    assert!(!draw.blend.flip);
    // Unadjusted, it stays square.
    assert_eq!(
        fixture.draw([2, 1], PLAIN).tile,
        [[4.0, 68.0], [36.0, 68.0], [36.0, 36.0], [4.0, 36.0]]
    );
    // A rise of 23 is a slope of 1.4375, below 1.5: square.
    fixture.corners([2, 1], [0, 0, 0, 23]);
    assert_eq!(
        fixture.draw([2, 1], ADJUSTED).tile,
        [[4.0, 68.0], [36.0, 68.0], [36.0, 36.0], [4.0, 36.0]]
    );

    // Cliff UVs of a tile of the same class: each corner's `u` and `v` in shares of the 2,048
    // texels, from the block's left, 4, and its bottom, 4 + 128 = 132.
    fixture.parts.cliffs = vec![CliffUv {
        tile: 12,
        corners: [
            [0.0, -0.03125],
            [0.0625, -0.03125],
            [0.0625, 0.0],
            [0.0, 0.0],
        ],
        flip: true,
        mutant: false,
    }];
    fixture.cell([3, 2]).cliff_uv = Some(0);
    fixture.corners([3, 2], [10, 0, 0, 0]);
    let draw = fixture.draw([3, 2], ADJUSTED);
    assert_eq!(
        draw.tile,
        [[4.0, 68.0], [132.0, 68.0], [132.0, 132.0], [4.0, 132.0]]
    );
    // Their flip asks for the split by heights: |10 − 0| is past |0 − 0|.
    assert!(draw.blend.flip);
    // Cliff UVs of another class's tile leave the tile as the heights give it, square on flat
    // ground.
    fixture.parts.cliffs[0].tile = 16;
    fixture.corners([3, 2], [0; 4]);
    assert_eq!(
        fixture.draw([3, 2], ADJUSTED).tile,
        [[4.0, 68.0], [36.0, 68.0], [36.0, 36.0], [4.0, 36.0]]
    );
}
