use bevy_math::UVec2;
use campfire_client_files::{BorderWrap, TerrainTexture, TextureCopy};

use crate::zero_hour::terrain::{ClassTexture, TerrainParts};

/// Zero Hour's terrain texture, as `WorldHeightMap` lays it out and `TerrainTextureClass` fills
/// it, as a terrain file's texture (design 16, D2): 2,048 texels wide, each texture class a
/// square block of its tiles, 64 texels each, placed on a grid of 72-texel cells, the widest
/// classes first, and wrapped about by a border of 4 texels, so a filtered or stretched sample
/// near a block's edge reads its own class. Its three levels, as the game keeps three: each texel
/// of a level below covers 2 × 2 of the one above within one tile, since tiles and borders start
/// on texels divisible by 4, so a tile's level is its texture's own level of that tile, and the
/// wrap of a level is the level of the wrap.
#[derive(Debug)]
pub(crate) struct TerrainAtlas {
    /// Each class's block's corner, by the class's index, when the grid had room for it.
    classes: Vec<Option<UVec2>>,
    /// Each source tile's corner, by its index, when its class's texture loaded it and the grid
    /// placed it: the game draws any other as no texture.
    tiles: Vec<Option<UVec2>>,
    /// Each placed tile copied from its class's texture, then each placed block's border.
    texture: TerrainTexture,
}

/// Each class's block's corner, by the class's index, when the grid had room for it; and the
/// texture's height.
#[derive(Debug)]
struct BlockPlaces {
    corners: Vec<Option<UVec2>>,
    height: u32,
}

/// Where a loaded source tile comes from: its class's texture, and its column and row of that
/// texture's tiles, rows from the texture's bottom, as the game reads a TGA's rows.
#[derive(Debug, Clone, Copy)]
struct TileSource<'a> {
    texture: &'a ClassTexture,
    column: u32,
    row: u32,
}

impl TerrainAtlas {
    pub(crate) const WIDTH: u32 = 2048;
    pub(crate) const TILE: u32 = 64;
    /// The space between two cells of the grid: a border on each side.
    const OFFSET: u32 = 8;
    const BORDER: u32 = TerrainAtlas::OFFSET / 2;
    /// The cells of a row of the grid, and of its rows.
    const CELLS: usize =
        (TerrainAtlas::WIDTH / (TerrainAtlas::TILE + TerrainAtlas::OFFSET)) as usize;
    const LEVELS: u32 = 3;
    /// The most tiles a side of a class's texture gives.
    const MOST_TILES: u32 = 10;

    /// The atlas of `terrain`'s classes and their textures.
    pub(crate) fn new(terrain: &TerrainParts) -> TerrainAtlas {
        let tiles = usize::try_from(terrain.tiles).expect("a u32 fits usize");
        let mut sources = vec![None; tiles];
        for class in &terrain.classes {
            let Some(texture) = &class.texture else {
                continue;
            };
            let Some(side) = TerrainAtlas::read_side(class.tiles, texture.size) else {
                continue;
            };
            for at in 0..side * side {
                sources[usize::try_from(class.first_tile + at).expect("a u32 fits usize")] =
                    Some(TileSource {
                        texture,
                        column: at % side,
                        row: at / side,
                    });
            }
        }
        let BlockPlaces {
            corners: classes,
            height,
        } = TerrainAtlas::place(terrain);
        let mut corners = vec![None; tiles];
        for (data, corner) in terrain.classes.iter().zip(&classes) {
            let Some(UVec2 { x, y }) = *corner else {
                continue;
            };
            for row in 0..data.width {
                for column in 0..data.width {
                    let at = usize::try_from(data.first_tile + column + row * data.width)
                        .expect("a u32 fits usize");
                    // A block wider than its class's tiles has squares no tile fills.
                    let Some(corner) = corners.get_mut(at) else {
                        continue;
                    };
                    *corner = Some(UVec2::new(
                        x + column * TerrainAtlas::TILE,
                        y + (data.width - row - 1) * TerrainAtlas::TILE,
                    ));
                }
            }
        }
        let tile = TerrainAtlas::TILE;
        let mut copies = Vec::new();
        let mut placed = Vec::with_capacity(tiles);
        for (corner, source) in corners.iter().zip(&sources) {
            let (Some(to), Some(source)) = (*corner, *source) else {
                placed.push(None);
                continue;
            };
            placed.push(Some(to));
            copies.push(TextureCopy {
                texture: source.texture.path.clone(),
                from: UVec2::new(
                    source.column * tile,
                    source.texture.size.y - tile * (source.row + 1),
                ),
                to,
                size: UVec2::splat(tile),
            });
        }
        let wraps = terrain
            .classes
            .iter()
            .zip(&classes)
            .filter_map(|(data, corner)| {
                Some(BorderWrap {
                    at: (*corner)?,
                    size: UVec2::splat(data.width * tile),
                    border: TerrainAtlas::BORDER,
                })
            })
            .collect();
        TerrainAtlas {
            classes,
            tiles: placed,
            texture: TerrainTexture {
                size: UVec2::new(TerrainAtlas::WIDTH, height),
                levels: TerrainAtlas::LEVELS,
                copies,
                wraps,
            },
        }
    }

    /// The top left corner of the source tile `tile`, in texels; none for a tile the game draws
    /// as no texture.
    pub(crate) fn tile(&self, tile: usize) -> Option<UVec2> {
        self.tiles.get(tile).copied().flatten()
    }

    /// The top left corner of the class at `class`'s block, in texels, `[0, 0]` for one the grid
    /// had no room for, as the game leaves it.
    pub(crate) fn class(&self, class: usize) -> UVec2 {
        self.classes[class].unwrap_or(UVec2::ZERO)
    }

    /// Its texture, as a terrain file gives it.
    pub(crate) fn into_texture(self) -> TerrainTexture {
        self.texture
    }

    /// How many tiles a side of a class of `tiles` loads from its texture of `size`, as
    /// `readTexClass` counts them: the texture gives the square of the fewer whole tiles of its
    /// sides, at most 10, and loads only when that holds the class's tiles, the largest square of
    /// them; none when it does not.
    fn read_side(tiles: u32, size: UVec2) -> Option<u32> {
        let given = (size / TerrainAtlas::TILE)
            .min_element()
            .min(TerrainAtlas::MOST_TILES);
        if tiles == 0 || given * given < tiles {
            return None;
        }
        (1..=TerrainAtlas::MOST_TILES)
            .rev()
            .find(|side| side * side <= tiles)
    }

    /// Each class's block's corner, as `updateTileTexturePositions` finds room: for each width,
    /// the widest first, each class of that width in order takes, row by row, the first row whose
    /// first free cell starts a free square of its width. A class with no room, or of a width
    /// past the grid, has none. The texture's height: the least power of two that holds every
    /// block and its border.
    fn place(terrain: &TerrainParts) -> BlockPlaces {
        let cells = TerrainAtlas::CELLS;
        let mut free = vec![[true; TerrainAtlas::CELLS]; cells];
        let mut corners = vec![None; terrain.classes.len()];
        let mut used = 0;
        for width in (1..=cells).rev() {
            for (class, data) in terrain.classes.iter().enumerate() {
                if usize::try_from(data.width) != Ok(width) {
                    continue;
                }
                let room = (0..=cells - width).find_map(|row| {
                    let column = (0..=cells - width).find(|&column| free[row][column])?;
                    let open = (0..width).all(|i| (0..width).all(|j| free[row + j][column + i]));
                    open.then_some([column, row])
                });
                let Some([column, row]) = room else {
                    continue;
                };
                for i in 0..width {
                    for j in 0..width {
                        free[row + j][column + i] = false;
                    }
                }
                let cell = TerrainAtlas::TILE + TerrainAtlas::OFFSET;
                let at = |index: usize| {
                    TerrainAtlas::BORDER + u32::try_from(index).expect("a cell of the grid") * cell
                };
                let [x, y] = [at(column), at(row)];
                corners[class] = Some(UVec2::new(x, y));
                used = used.max(y + data.width * TerrainAtlas::TILE + TerrainAtlas::BORDER);
            }
        }
        BlockPlaces {
            corners,
            height: used.max(1).next_power_of_two(),
        }
    }
}

#[cfg(test)]
mod tests;
