use crate::zero_hour::terrain::{TerrainCell, TextureClass};

use super::*;

/// A class of `width` tiles a side and `tiles` tiles from `first_tile`.
fn class(first_tile: u32, tiles: u32, width: u32) -> TextureClass {
    TextureClass {
        first_tile,
        tiles,
        width,
        name: String::new(),
        texture: None,
    }
}

/// A terrain of `classes`, holding their tiles, with one cell.
fn terrain(classes: Vec<TextureClass>) -> TerrainParts {
    let tiles = classes
        .iter()
        .map(|class| class.first_tile + class.tiles)
        .max()
        .unwrap_or(0);
    TerrainParts {
        columns: 1,
        cells: vec![TerrainCell {
            tile: 0,
            blend: None,
            extra_blend: None,
            cliff_uv: None,
            cliff: false,
        }],
        tiles,
        classes,
        edge_tiles: 0,
        edge_classes: Vec::new(),
        blends: Vec::new(),
        cliffs: Vec::new(),
    }
}

/// A texture `side` texels square of three levels, each texel `[x, y, level, 255]`, `y` from its
/// top.
fn texture(side: u32) -> Vec<u8> {
    (0..3_u32)
        .flat_map(|level| {
            let side = side >> level;
            (0..side).flat_map(move |y| {
                (0..side).flat_map(move |x| [byte(x), byte(y), byte(level), 255])
            })
        })
        .collect()
}

fn byte(value: u32) -> u8 {
    u8::try_from(value).unwrap()
}

/// The texel at `[x, y]` of level `level`.
fn texel(atlas: &TerrainAtlas, level: usize, [x, y]: [u32; 2]) -> [u8; 4] {
    let width = (TerrainAtlas::WIDTH >> level) as usize;
    let at = (y as usize * width + x as usize) * 4;
    atlas.levels[level][at..at + 4].try_into().unwrap()
}

#[test]
fn classes_take_the_grid_widest_first_and_one_with_no_room_has_none() {
    // Widths 1, 2, 1 and 3: the 3 takes the first cell, the 2 the next free one in row 0, and the
    // ones the free cells after it, in their order.
    let small = terrain(vec![
        class(0, 1, 1),
        class(1, 4, 2),
        class(5, 1, 1),
        class(6, 9, 3),
    ]);
    let (corners, height) = TerrainAtlas::place(&small);
    let cell = |column: u32, row: u32| Some([4 + 72 * column, 4 + 72 * row]);
    assert_eq!(corners, [cell(5, 0), cell(3, 0), cell(6, 0), cell(0, 0)]);
    // The 3's block and border reach 4 + 192 + 4 = 200 texels down: the next power of two.
    assert_eq!(height, 256);

    // A class of 27 leaves no free column of rows 0 to 26 that a 2 may start in, and no row
    // below them that a 2 fits in, so it has no room, but a 1 takes column 27 of row 0. A 29 is
    // wider than the grid.
    let crowded = terrain(vec![
        class(0, 1, 29),
        class(1, 4, 2),
        class(5, 729, 27),
        class(734, 1, 1),
    ]);
    let (corners, height) = TerrainAtlas::place(&crowded);
    assert_eq!(corners, [None, None, cell(0, 0), cell(27, 0)]);
    // The 27 reaches 4 + 27 · 64 + 4 = 1736 texels down.
    assert_eq!(height, 2048);
}

#[test]
fn a_class_s_tiles_lie_upright_in_its_block_wrapped_by_its_border_at_each_level() {
    let parts = terrain(vec![class(0, 4, 2)]);
    let data = texture(128);
    let textures = [Some(ClassTexture {
        width: 128,
        height: 128,
        data: data.clone(),
    })];
    let atlas = TerrainAtlas::new(&parts, &textures);
    let [x, y] = atlas.class(0);
    assert_eq!([x, y], [4, 4]);
    // The game reads a TGA's rows from its bottom: tile 0 is the bottom left, which the block's
    // bottom left holds, and tile 2 the top left.
    assert_eq!(atlas.tile(0), Some([4, 68]));
    assert_eq!(atlas.tile(1), Some([68, 68]));
    assert_eq!(atlas.tile(2), Some([4, 4]));
    assert_eq!(atlas.tile(3), Some([68, 4]));
    // So the block is the texture as it stands, texel for texel.
    for [i, j] in [[0, 0], [127, 0], [5, 70], [127, 127]] {
        assert_eq!(texel(&atlas, 0, [x + i, y + j]), [byte(i), byte(j), 0, 255]);
    }
    // Its border wraps: left of column 0 is column 127, above row 0 is row 127, and a corner
    // takes the opposite corner.
    assert_eq!(texel(&atlas, 0, [x - 1, y + 9]), [127, 9, 0, 255]);
    assert_eq!(texel(&atlas, 0, [x - 4, y + 9]), [124, 9, 0, 255]);
    assert_eq!(texel(&atlas, 0, [x + 128, y + 9]), [0, 9, 0, 255]);
    assert_eq!(texel(&atlas, 0, [x + 131, y + 9]), [3, 9, 0, 255]);
    assert_eq!(texel(&atlas, 0, [x + 9, y - 1]), [9, 127, 0, 255]);
    assert_eq!(texel(&atlas, 0, [x + 9, y + 128]), [9, 0, 0, 255]);
    assert_eq!(texel(&atlas, 0, [x - 1, y - 1]), [127, 127, 0, 255]);
    assert_eq!(texel(&atlas, 0, [x + 131, y + 131]), [3, 3, 0, 255]);
    // Past the border, nothing.
    assert_eq!(texel(&atlas, 0, [x + 132, y]), [0; 4]);
    // Level 1 halves it all: the block from 2, a border of 2.
    assert_eq!(texel(&atlas, 1, [2 + 63, 2 + 10]), [63, 10, 1, 255]);
    assert_eq!(texel(&atlas, 1, [2 - 2, 2 + 10]), [62, 10, 1, 255]);
    // Level 2: a block from 1, a border of 1.
    assert_eq!(texel(&atlas, 2, [1 + 31, 1 + 31]), [31, 31, 2, 255]);
    assert_eq!(texel(&atlas, 2, [0, 1]), [31, 0, 2, 255]);
    assert_eq!(
        atlas.into_texels().len(),
        (2048 * 256 + 1024 * 128 + 512 * 64) * 4
    );
}

#[test]
fn a_class_loads_the_largest_square_of_its_tiles_that_its_texture_holds() {
    // Three tiles load a square of one: the bottom left, at the block's bottom left; tiles 1 and
    // 2 have no texture.
    let data = texture(128);
    let three = terrain(vec![class(0, 3, 2)]);
    let textures = [Some(ClassTexture {
        width: 128,
        height: 128,
        data: data.clone(),
    })];
    let atlas = TerrainAtlas::new(&three, &textures);
    assert_eq!(atlas.tile(0), Some([4, 68]));
    assert_eq!(atlas.tile(1), None);
    assert_eq!(atlas.tile(2), None);
    assert_eq!(texel(&atlas, 0, [4, 68]), [0, 64, 0, 255]);
    // A texture of one tile holds no class of four, and a class with no texture loads none, but
    // keeps its block.
    let small = texture(64);
    let four = terrain(vec![class(0, 4, 2), class(4, 1, 1)]);
    let textures = [
        Some(ClassTexture {
            width: 64,
            height: 64,
            data: small.clone(),
        }),
        None,
    ];
    let atlas = TerrainAtlas::new(&four, &textures);
    assert_eq!(
        (0..5).map(|tile| atlas.tile(tile)).collect::<Vec<_>>(),
        [None; 5]
    );
    assert_eq!(atlas.class(1), [4 + 144, 4]);
    // Too few levels of bytes load nothing either.
    let short = TerrainAtlas::read_side(
        1,
        &ClassTexture {
            width: 64,
            height: 64,
            data: small[..64 * 64 * 4].to_vec(),
        },
    );
    assert_eq!(short, None);
}
