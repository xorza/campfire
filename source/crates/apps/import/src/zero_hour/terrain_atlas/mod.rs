use std::cmp::Reverse;

use bevy_math::UVec2;
use campfire_client_files::{BorderWrap, TerrainTexture, TextureCopy};

use campfire_values::PackagePath;

use crate::zero_hour::road_drop_cause::RoadDropCause;

use crate::zero_hour::format::map::terrain::{Terrain, TerrainFields};

/// Zero Hour's terrain texture, as `WorldHeightMap` lays it out and `TerrainTextureClass` fills it,
/// as a terrain file's texture (02-engine-core.md, The terrain's texture is composed at load):
/// 2,048 texels wide, each texture class a square block of its tiles, 64 texels each, placed on a
/// grid of 72-texel cells, the widest classes first, and wrapped about by a border of 4 texels, so
/// a filtered or stretched sample near a block's edge reads its own class. Its three levels, as the
/// game keeps three: each texel of a level below covers 2 × 2 of the one above within one tile,
/// since tiles and borders start on texels divisible by 4, so a tile's level is its texture's own
/// level of that tile, and the wrap of a level is the level of the wrap. Beside the classes, when
/// a map draws roads, the texture grows to twice the width, and each road type's texture takes a
/// place of its own there, wrapped about by a border alike, so a road that repeats its texture
/// samples its own copy (02-engine-core.md, A decal's texture joins the terrain's).
#[derive(Debug)]
pub(crate) struct TerrainAtlas {
    /// Each class's block's corner, by the class's index, when the grid had room for it.
    classes: Vec<Option<UVec2>>,
    /// Each source tile's corner, by its index, when its class's texture loaded it and the grid
    /// placed it: the game draws any other as no texture.
    tiles: Vec<Option<UVec2>>,
    /// Each road texture's place, by its index, or why it has none.
    roads: Vec<Result<TexturePlace, RoadDropCause>>,
    /// Each placed tile copied from its class's texture, then each placed road texture; each
    /// placed block's border, then each road texture's.
    texture: TerrainTexture,
}

/// Each class's block's corner, by the class's index, when the grid had room for it; and the
/// texture's height.
#[derive(Debug)]
struct BlockPlaces {
    corners: Vec<Option<UVec2>>,
    height: u32,
}

/// Each road texture's corner, by its index, when it has a place; and the height its places
/// take.
#[derive(Debug)]
struct RoadPlaces {
    corners: Vec<Option<UVec2>>,
    height: u32,
}

/// A road texture's copy in the terrain's texture: its top left corner and its size, in texels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TexturePlace {
    pub(crate) corner: UVec2,
    pub(crate) size: UVec2,
}

/// A texture of the package: its file, and its size in texels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PackageTexture {
    pub(crate) path: PackagePath,
    pub(crate) size: UVec2,
}

/// Where a loaded source tile comes from: its class's texture, and its column and row of that
/// texture's tiles, rows from the texture's bottom, as the game reads a TGA's rows.
#[derive(Debug, Clone, Copy)]
struct TileSource<'a> {
    texture: &'a PackageTexture,
    column: u32,
    row: u32,
}

/// The class width the map format bounds is the one the grid holds.
const _: () = assert!(TerrainAtlas::CELLS == Terrain::MOST_CLASS_WIDTH as usize);

impl TerrainAtlas {
    pub(crate) const WIDTH: u32 = 2048;
    pub(crate) const TILE: u32 = 64;
    /// The space between two cells of the grid: a border on each side.
    const OFFSET: u32 = 8;
    const BORDER: u32 = Self::OFFSET / 2;
    /// The cells of a row of the grid, and of its rows.
    const CELLS: usize = (Self::WIDTH / (Self::TILE + Self::OFFSET)) as usize;
    const LEVELS: u32 = 3;
    /// The most tiles a side of a class's texture gives.
    const MOST_TILES: u32 = 10;

    /// The atlas of `terrain`'s classes and `textures`, the texture of each class in its order,
    /// none for a class `Terrain.ini` names none of, or a TGA the install lacks, which the game
    /// draws as no texture; and of `roads`, the textures of a map's road types, none for one no
    /// archive holds.
    pub(crate) fn new(
        terrain: &TerrainFields,
        textures: &[Option<PackageTexture>],
        roads: &[Option<PackageTexture>],
    ) -> Self {
        debug_assert_eq!(
            terrain.classes.len(),
            textures.len(),
            "a texture for each class"
        );
        let tiles = usize::try_from(terrain.tiles).expect("a u32 fits usize");
        let mut sources = vec![None; tiles];
        for (class, texture) in terrain.classes.iter().zip(textures) {
            let Some(texture) = texture else {
                continue;
            };
            let Some(side) = Self::read_side(class.tiles, texture.size) else {
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
        } = Self::place(terrain);
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
                        x + column * Self::TILE,
                        y + (data.width - row - 1) * Self::TILE,
                    ));
                }
            }
        }
        let tile = Self::TILE;
        let mut copies = Vec::with_capacity(tiles + roads.len());
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
        let mut wraps: Vec<BorderWrap> = terrain
            .classes
            .iter()
            .zip(&classes)
            .filter_map(|(data, corner)| {
                Some(BorderWrap {
                    at: (*corner)?,
                    size: UVec2::splat(data.width * tile),
                    border: Self::BORDER,
                })
            })
            .collect();
        let RoadPlaces {
            corners: road_corners,
            height: road_height,
        } = Self::place_roads(roads);
        for (texture, corner) in roads.iter().zip(&road_corners) {
            let (Some(texture), Some(at)) = (texture, *corner) else {
                continue;
            };
            copies.push(TextureCopy {
                texture: texture.path.clone(),
                from: UVec2::ZERO,
                to: at,
                size: texture.size,
            });
            wraps.push(BorderWrap {
                at,
                size: texture.size,
                border: Self::BORDER,
            });
        }
        let width = if road_corners.iter().any(Option::is_some) {
            2 * Self::WIDTH
        } else {
            Self::WIDTH
        };
        let road_places = roads
            .iter()
            .zip(road_corners)
            .map(|(texture, corner)| {
                let size = texture.as_ref().ok_or(RoadDropCause::NoTexture)?.size;
                let corner = corner.ok_or(RoadDropCause::NoPlace)?;
                Ok(TexturePlace { corner, size })
            })
            .collect();
        Self {
            classes,
            tiles: placed,
            roads: road_places,
            texture: TerrainTexture {
                size: UVec2::new(width, height.max(road_height)),
                levels: Self::LEVELS,
                copies,
                wraps,
            },
        }
    }

    /// The place of each road texture it was made of, in their order, or why one has none: no
    /// texture, or no place.
    pub(crate) fn roads(&self) -> &[Result<TexturePlace, RoadDropCause>] {
        &self.roads
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
        let given = (size / Self::TILE).min_element().min(Self::MOST_TILES);
        if tiles == 0 || given * given < tiles {
            return None;
        }
        (1..=Self::MOST_TILES)
            .rev()
            .find(|side| side * side <= tiles)
    }

    /// Each road texture's corner beside the classes' grid, from `WIDTH` across, in rows: the
    /// tallest first, then the widest, then in their order, each the next in its row while the
    /// row has room, the next row under the row's tallest, `OFFSET` between each two and a
    /// `BORDER` past the outermost. A texture whose sides the levels do not halve to whole
    /// texels, or one past `TerrainTexture::MOST_SIDE`, has none. The least power of two that
    /// holds every placed one and its border.
    fn place_roads(roads: &[Option<PackageTexture>]) -> RoadPlaces {
        let step = 1 << (Self::LEVELS - 1);
        let mut order: Vec<(usize, UVec2)> = roads
            .iter()
            .enumerate()
            .filter_map(|(at, texture)| {
                let size = texture.as_ref()?.size;
                (size.x.is_multiple_of(step) && size.y.is_multiple_of(step) && size.x > 0)
                    .then_some((at, size))
            })
            .collect();
        order.sort_by_key(|&(at, size)| (Reverse(size.y), Reverse(size.x), at));
        let mut corners = vec![None; roads.len()];
        let [left, right] = [Self::WIDTH, 2 * Self::WIDTH].map(|x| x + Self::BORDER);
        let right = right - 2 * Self::BORDER;
        let mut at = UVec2::new(left, Self::BORDER);
        let mut row_height = 0;
        let mut used = 0;
        for (road, size) in order {
            if at.x + size.x > right && at.x > left {
                at = UVec2::new(left, at.y + row_height + Self::OFFSET);
                row_height = 0;
            }
            let fits =
                at.x + size.x <= right && at.y + size.y + Self::BORDER <= TerrainTexture::MOST_SIDE;
            if !fits {
                continue;
            }
            corners[road] = Some(at);
            row_height = row_height.max(size.y);
            used = used.max(at.y + size.y + Self::BORDER);
            at.x += size.x + Self::OFFSET;
        }
        RoadPlaces {
            corners,
            height: used.max(1).next_power_of_two(),
        }
    }

    /// Each class's block's corner, as `updateTileTexturePositions` finds room: for each width,
    /// the widest first, each class of that width in order takes, row by row, the first row whose
    /// first free cell starts a free square of its width. A class with no room, or of a width
    /// past the grid, has none. The texture's height: the least power of two that holds every
    /// block and its border.
    fn place(terrain: &TerrainFields) -> BlockPlaces {
        let cells = Self::CELLS;
        let mut free = vec![[true; Self::CELLS]; cells];
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
                let cell = Self::TILE + Self::OFFSET;
                let at = |index: usize| {
                    Self::BORDER + u32::try_from(index).expect("a cell of the grid") * cell
                };
                let [x, y] = [at(column), at(row)];
                corners[class] = Some(UVec2::new(x, y));
                used = used.max(y + data.width * Self::TILE + Self::BORDER);
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
