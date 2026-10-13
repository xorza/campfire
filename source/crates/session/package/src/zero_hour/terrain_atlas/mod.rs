use crate::zero_hour::class_texture::ClassTexture;
use crate::zero_hour::terrain::TerrainParts;

/// Zero Hour's terrain texture, as `WorldHeightMap` lays it out and `TerrainTextureClass` fills
/// it: 2,048 texels wide, each texture class a square block of its tiles, 64 texels each, placed
/// on a grid of 72-texel cells, the widest classes first, and wrapped about by a border of 4
/// texels, so a filtered or stretched sample near a block's edge reads its own class. Its three
/// levels, as the game keeps three: each texel of a level below covers 2 × 2 of the one above
/// within one tile, since tiles and borders start on texels divisible by 4, so a tile's level is
/// its texture's own level of that tile, and the wrap of a level is the level of the wrap.
#[derive(Debug)]
pub struct TerrainAtlas {
    height: u32,
    /// Each class's block's corner, by the class's index, when the grid had room for it.
    classes: Vec<Option<[u32; 2]>>,
    /// Each source tile's corner, by its index, when its class's texture loaded it and the grid
    /// placed it: the game draws any other as no texture.
    tiles: Vec<Option<[u32; 2]>>,
    /// Its three levels, each rows from the top of red, green, blue and alpha.
    levels: [Vec<u8>; 3],
}

/// Where a loaded source tile comes from: its class's texture, and its column and row of that
/// texture's tiles, rows from the texture's bottom, as the game reads a TGA's rows.
#[derive(Debug, Clone, Copy)]
struct TileSource {
    class: usize,
    column: u32,
    row: u32,
}

impl TerrainAtlas {
    pub const WIDTH: u32 = 2048;
    pub const TILE: u32 = 64;
    /// The space between two cells of the grid: a border on each side.
    const OFFSET: u32 = 8;
    const BORDER: u32 = TerrainAtlas::OFFSET / 2;
    /// The cells of a row of the grid, and of its rows.
    const CELLS: usize =
        (TerrainAtlas::WIDTH / (TerrainAtlas::TILE + TerrainAtlas::OFFSET)) as usize;
    const LEVELS: u32 = 3;
    /// The most tiles a side of a class's texture gives.
    const MOST_TILES: u32 = 10;

    /// The atlas of `terrain`'s classes, with `textures`, each class's by its index.
    pub fn new(terrain: &TerrainParts, textures: &[Option<ClassTexture>]) -> TerrainAtlas {
        let tiles = terrain.tiles as usize;
        let mut sources = vec![None; tiles];
        for (class, (data, texture)) in terrain.classes.iter().zip(textures).enumerate() {
            let Some(texture) = texture else {
                continue;
            };
            let Some(side) = TerrainAtlas::read_side(data.tiles, texture) else {
                continue;
            };
            for at in 0..side * side {
                sources[(data.first_tile + at) as usize] = Some(TileSource {
                    class,
                    column: at % side,
                    row: at / side,
                });
            }
        }
        let (classes, height) = TerrainAtlas::place(terrain);
        let mut corners = vec![None; tiles];
        for (data, corner) in terrain.classes.iter().zip(&classes) {
            let Some([x, y]) = *corner else {
                continue;
            };
            for row in 0..data.width {
                for column in 0..data.width {
                    let at = (data.first_tile + column + row * data.width) as usize;
                    // A block wider than its class's tiles has squares no tile fills.
                    let Some(corner) = corners.get_mut(at) else {
                        continue;
                    };
                    *corner = Some([
                        x + column * TerrainAtlas::TILE,
                        y + (data.width - row - 1) * TerrainAtlas::TILE,
                    ]);
                }
            }
        }
        let placed: Vec<Option<[u32; 2]>> = corners
            .iter()
            .zip(&sources)
            .map(|(corner, source)| corner.filter(|_| source.is_some()))
            .collect();
        let levels = [0, 1, 2].map(|level| {
            TerrainAtlas::paint(
                terrain, &classes, &placed, &sources, textures, height, level,
            )
        });
        TerrainAtlas {
            height,
            classes,
            tiles: placed,
            levels,
        }
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    /// The top left corner of the source tile `tile`, in texels; none for a tile the game draws
    /// as no texture.
    pub fn tile(&self, tile: usize) -> Option<[u32; 2]> {
        self.tiles.get(tile).copied().flatten()
    }

    /// The top left corner of the class at `class`'s block, in texels, `[0, 0]` for one the grid
    /// had no room for, as the game leaves it.
    pub fn class(&self, class: usize) -> [u32; 2] {
        self.classes[class].unwrap_or([0, 0])
    }

    /// Its levels, the largest first, one after another.
    pub fn into_texels(self) -> Vec<u8> {
        self.levels.concat()
    }

    /// How many tiles a side of a class of `tiles` loads from `texture`, as `readTexClass` counts
    /// them: the texture gives the square of the fewer whole tiles of its sides, at most 10, and
    /// loads only when that holds the class's tiles, the largest square of them; none when it
    /// does not, or for a texture of fewer than the three levels the atlas takes.
    fn read_side(tiles: u32, texture: &ClassTexture) -> Option<u32> {
        let given = (texture.width / TerrainAtlas::TILE)
            .min(texture.height / TerrainAtlas::TILE)
            .min(TerrainAtlas::MOST_TILES);
        let levels: usize = (0..TerrainAtlas::LEVELS)
            .map(|level| ((texture.width >> level) * (texture.height >> level) * 4) as usize)
            .sum();
        if tiles == 0 || given * given < tiles || texture.data.len() < levels {
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
    fn place(terrain: &TerrainParts) -> (Vec<Option<[u32; 2]>>, u32) {
        let cells = TerrainAtlas::CELLS;
        let mut free = vec![[true; TerrainAtlas::CELLS]; cells];
        let mut corners = vec![None; terrain.classes.len()];
        let mut used = 0;
        for width in (1..=cells).rev() {
            for (class, data) in terrain.classes.iter().enumerate() {
                if data.width as usize != width {
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
                corners[class] = Some([x, y]);
                used = used.max(y + data.width * TerrainAtlas::TILE + TerrainAtlas::BORDER);
            }
        }
        (corners, used.max(1).next_power_of_two())
    }

    /// The atlas's level `level`: each placed tile from its texture's level, then each placed
    /// class's border, its columns and then its rows, wrapped from its block's other side.
    fn paint(
        terrain: &TerrainParts,
        classes: &[Option<[u32; 2]>],
        placed: &[Option<[u32; 2]>],
        sources: &[Option<TileSource>],
        textures: &[Option<ClassTexture>],
        height: u32,
        level: u32,
    ) -> Vec<u8> {
        let width = (TerrainAtlas::WIDTH >> level) as usize;
        let rows = (height >> level).max(1) as usize;
        let tile = (TerrainAtlas::TILE >> level) as usize;
        let mut texels = vec![0_u8; width * rows * 4];
        for (corner, source) in placed.iter().zip(sources) {
            let (Some([x, y]), Some(source)) = (*corner, *source) else {
                continue;
            };
            let texture = textures[source.class]
                .as_ref()
                .expect("a loaded tile's class has its texture");
            let side = (texture.width >> level) as usize;
            let start: usize = (0..level)
                .map(|above| ((texture.width >> above) * (texture.height >> above) * 4) as usize)
                .sum();
            let bottom = (texture.height >> level) as usize;
            let [x, y] = [(x >> level) as usize, (y >> level) as usize];
            for line in 0..tile {
                let from_row = bottom - tile * (source.row as usize + 1) + line;
                let from = start + (from_row * side + source.column as usize * tile) * 4;
                let to = ((y + line) * width + x) * 4;
                texels[to..to + tile * 4].copy_from_slice(&texture.data[from..from + tile * 4]);
            }
        }
        let border = (TerrainAtlas::BORDER >> level) as usize;
        for (data, corner) in terrain.classes.iter().zip(classes) {
            let Some([x, y]) = *corner else {
                continue;
            };
            let [x, y] = [(x >> level) as usize, (y >> level) as usize];
            let span = data.width as usize * tile;
            let texel = |column: usize, row: usize| (row * width + column) * 4;
            for row in y..y + span {
                texels.copy_within(
                    texel(x + span - border, row)..texel(x + span, row),
                    texel(x - border, row),
                );
                texels.copy_within(texel(x, row)..texel(x + border, row), texel(x + span, row));
            }
            for line in 0..border {
                let wide = |row: usize| texel(x - border, row)..texel(x + span + border, row);
                texels.copy_within(wide(y + span - 1 - line), texel(x - border, y - 1 - line));
                texels.copy_within(wide(y + line), texel(x - border, y + span + line));
            }
        }
        texels
    }
}

#[cfg(test)]
mod tests;
