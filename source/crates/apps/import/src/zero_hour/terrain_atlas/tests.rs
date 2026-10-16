use super::*;
use crate::zero_hour::format::map::terrain::{Terrain, TerrainCell, TextureClass};

/// A class of `width` tiles a side and `tiles` tiles from `first_tile`.
fn class(first_tile: u32, tiles: u32, width: u32) -> TextureClass {
    TextureClass {
        first_tile,
        tiles,
        width,
        name: String::new(),
    }
}

/// A class's texture of `side` × `side` texels at `path`.
fn texture(path: &str, side: u32) -> ClassTexture {
    ClassTexture {
        path: PackagePath::parse(path).unwrap(),
        size: UVec2::splat(side),
    }
}

/// A terrain of `classes`, holding their tiles, with one cell.
fn terrain(classes: impl IntoIterator<Item = TextureClass>) -> TerrainParts {
    let classes: Vec<TextureClass> = classes.into_iter().collect();
    let tiles = classes
        .iter()
        .map(|class| class.first_tile + class.tiles)
        .max()
        .unwrap_or(0);
    TerrainParts {
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

/// A copy of the tile of `texture` from `from` to `to`.
fn copy(texture: &str, [x, y]: [u32; 2], [to_x, to_y]: [u32; 2]) -> TextureCopy {
    TextureCopy {
        texture: PackagePath::parse(texture).unwrap(),
        from: UVec2::new(x, y),
        to: UVec2::new(to_x, to_y),
        size: UVec2::splat(64),
    }
}

#[test]
fn classes_take_the_grid_widest_first_and_one_with_no_room_has_none() {
    // Widths 1, 2, 1 and 3: the 3 takes the first cell, the 2 the next free one in row 0, and the
    // ones the free cells after it, in their order.
    let small = terrain([
        class(0, 1, 1),
        class(1, 4, 2),
        class(5, 1, 1),
        class(6, 9, 3),
    ]);
    let BlockPlaces { corners, height } = TerrainAtlas::place(&small);
    let cell = |column: u32, row: u32| Some(UVec2::new(4 + 72 * column, 4 + 72 * row));
    assert_eq!(corners, [cell(5, 0), cell(3, 0), cell(6, 0), cell(0, 0)]);
    // The 3's block and border reach 4 + 192 + 4 = 200 texels down: the next power of two.
    assert_eq!(height, 256);

    // A class of 27 leaves no free column of rows 0 to 26 that a 2 may start in, and no row
    // below them that a 2 fits in, so it has no room, but a 1 takes column 27 of row 0. A 29 is
    // wider than the grid.
    let crowded = terrain([
        class(0, 1, 29),
        class(1, 4, 2),
        class(5, 729, 27),
        class(734, 1, 1),
    ]);
    let BlockPlaces { corners, height } = TerrainAtlas::place(&crowded);
    assert_eq!(corners, [None, None, cell(0, 0), cell(27, 0)]);
    // The 27 reaches 4 + 27 · 64 + 4 = 1736 texels down.
    assert_eq!(height, 2048);
}

#[test]
fn a_class_s_tiles_lie_upright_in_its_block_wrapped_by_its_border() {
    const SAND: &str = "client/textures/art/terrain/sand.ktx2";
    let sand = class(0, 4, 2);
    let atlas = TerrainAtlas::new(&terrain([sand]), &[Some(texture(SAND, 128))]);
    assert_eq!(atlas.class(0), UVec2::new(4, 4));
    // The game reads a TGA's rows from its bottom: tile 0 is the bottom left, which the block's
    // bottom left holds, and tile 2 the top left.
    assert_eq!(atlas.tile(0), Some(UVec2::new(4, 68)));
    assert_eq!(atlas.tile(1), Some(UVec2::new(68, 68)));
    assert_eq!(atlas.tile(2), Some(UVec2::new(4, 4)));
    assert_eq!(atlas.tile(3), Some(UVec2::new(68, 4)));
    // So each tile copies from its own place, rows from the texture's top: the block is the
    // texture as it stands, texel for texel. Its border wraps 4 texels about the block, and the
    // texture is 2,048 wide and 256 high, the block and its border reaching 4 + 128 + 4 = 136.
    assert_eq!(
        atlas.into_texture(),
        TerrainTexture {
            size: UVec2::new(2048, 256),
            levels: 3,
            copies: vec![
                copy(SAND, [0, 64], [4, 68]),
                copy(SAND, [64, 64], [68, 68]),
                copy(SAND, [0, 0], [4, 4]),
                copy(SAND, [64, 0], [68, 4]),
            ],
            wraps: vec![BorderWrap {
                at: UVec2::new(4, 4),
                size: UVec2::splat(128),
                border: 4,
            }],
        }
    );
}

#[test]
fn a_class_loads_the_largest_square_of_its_tiles_that_its_texture_holds() {
    const SAND: &str = "client/textures/art/terrain/sand.ktx2";
    // Three tiles load a square of one: the bottom left, at the block's bottom left; tiles 1 and
    // 2 have no texture.
    let three = class(0, 3, 2);
    let atlas = TerrainAtlas::new(&terrain([three]), &[Some(texture(SAND, 128))]);
    assert_eq!(atlas.tile(0), Some(UVec2::new(4, 68)));
    assert_eq!(atlas.tile(1), None);
    assert_eq!(atlas.tile(2), None);
    assert_eq!(atlas.into_texture().copies, [copy(SAND, [0, 64], [4, 68])]);
    // A texture of one tile holds no class of four, and a class with no texture loads none, but
    // keeps its block and its border.
    let four = class(0, 4, 2);
    let atlas = TerrainAtlas::new(
        &terrain([four, class(4, 1, 1)]),
        &[Some(texture(SAND, 64)), None],
    );
    assert_eq!(
        (0..5).map(|tile| atlas.tile(tile)).collect::<Vec<_>>(),
        [None; 5]
    );
    assert_eq!(atlas.class(1), UVec2::new(4 + 144, 4));
    let texture = atlas.into_texture();
    assert!(texture.copies.is_empty());
    assert_eq!(texture.wraps.len(), 2);
    // A texture narrower than a tile loads none either.
    assert_eq!(TerrainAtlas::read_side(1, UVec2::splat(64)), Some(1));
    assert_eq!(TerrainAtlas::read_side(1, UVec2::new(63, 64)), None);
}

#[test]
fn a_class_the_terrain_holds_fits_the_atlas_grid() {
    // The terrain refuses a class wider than the grid's cells a row, and the grid places one
    // that wide: 28 cells of 72 texels, 2,016, then a border, within 2,048.
    assert_eq!(
        usize::try_from(Terrain::MOST_CLASS_WIDTH).unwrap(),
        TerrainAtlas::CELLS
    );
    let widest = TerrainAtlas::place(&terrain([class(0, 28 * 28, 28)]));
    assert_eq!(widest.corners, [Some(UVec2::new(4, 4))]);
}
