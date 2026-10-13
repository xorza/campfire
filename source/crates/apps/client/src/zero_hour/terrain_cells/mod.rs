use campfire_package::{BlendShape, BlendTile, GameData, TerrainAtlas, TerrainCell, TerrainParts};

/// What each cell of Zero Hour's terrain draws, as `WorldHeightMap` gives it to the heightmap's
/// renderer, in `f32` as the game computes it. A cell is named on the game's axes, `[x, y]`, `x`
/// east and `y` north, its corners 0 to 3 at its south-west, south-east, north-east and
/// north-west, the game's order; texture coordinates are texels of the atlas, `u` right and `v`
/// down.
#[derive(Debug)]
pub(crate) struct TerrainCells<'a> {
    terrain: &'a TerrainParts,
    /// The heights, rows from the game's north, the package's order.
    samples: &'a [u8],
    columns: usize,
    rows: usize,
    atlas: &'a TerrainAtlas,
    game: GameData,
}

/// A cell's layers: its tile; its blend over the tile, alpha 0 at every corner for a cell with
/// none, whose split the tile takes too; and its third blend over both, with its own split,
/// when it has one and the game draws them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CellDraw {
    pub(crate) tile: [[f32; 2]; 4],
    pub(crate) blend: Layer,
    pub(crate) extra: Option<Layer>,
}

/// A layer of a cell: its texture coordinates and its alpha at each corner, and whether its two
/// triangles meet on the diagonal from corner 1 to corner 3, not from 0 to 2.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Layer {
    pub(crate) uv: [[f32; 2]; 4],
    pub(crate) alpha: [u8; 4],
    pub(crate) flip: bool,
}

/// A tile's coordinates over a cell, and whether a cliff stretched them.
#[derive(Debug, Clone, Copy)]
struct TileUv {
    uv: [[f32; 2]; 4],
    stretched: bool,
}

/// A blend's alpha at each corner, and whether it flips its cell.
#[derive(Debug, Clone, Copy)]
struct Pattern {
    alpha: [u8; 4],
    flip: bool,
}

/// The game's height step over its cell's side: a cell's slope per step of height.
const HEIGHT_SCALE: f32 = 0.625 / 10.0;
/// A slope below which a cell's tile keeps its square.
const STRETCH_LIMIT: f32 = 1.5;
/// The most tiles a stretch covers.
const TILE_LIMIT: f32 = 4.0;
const TALL_STRETCH_LIMIT: f32 = 2.0;
const DIAMOND_STRETCH_LIMIT: f32 = 2.4;
const OPAQUE: u8 = 255;

impl<'a> TerrainCells<'a> {
    /// The cells of `terrain` over `samples`, `columns` wide, with `atlas` and `game`'s settings.
    pub(crate) fn new(
        terrain: &'a TerrainParts,
        samples: &'a [u8],
        atlas: &'a TerrainAtlas,
        game: GameData,
    ) -> TerrainCells<'a> {
        let columns = terrain.columns as usize;
        debug_assert_eq!(samples.len(), terrain.cells.len());
        TerrainCells {
            terrain,
            samples,
            columns,
            rows: samples.len() / columns,
            atlas,
            game,
        }
    }

    /// The cells the game draws on each axis: all but the last sample's.
    pub(crate) const fn drawn(&self) -> [usize; 2] {
        [self.columns - 1, self.rows - 1]
    }

    /// The height of the sample `[x, y]`, in steps.
    pub(crate) fn height(&self, [x, y]: [usize; 2]) -> i32 {
        i32::from(self.samples[self.at([x, y])])
    }

    /// What the cell `[x, y]` draws, as `getUVData`, `getAlphaUVData` and
    /// `getExtraAlphaUVData` give it.
    pub(crate) fn cell(&self, at: [usize; 2]) -> CellDraw {
        let cell = self.terrain_cell(at);
        let tile = self.tile_uv(at, cell.tile).uv;
        let (blend, stretched) = match cell.blend {
            None => {
                let own = self.tile_uv(at, cell.tile);
                let layer = Layer {
                    uv: own.uv,
                    alpha: [0; 4],
                    flip: false,
                };
                (layer, own.stretched)
            }
            Some(blend) => {
                let blend = &self.terrain.blends[usize::from(blend)];
                let over = self.tile_uv(at, blend.tile);
                let pattern = self.pattern(blend);
                let layer = Layer {
                    uv: over.uv,
                    alpha: pattern.alpha,
                    flip: pattern.flip,
                };
                (layer, over.stretched)
            }
        };
        let crossed = self.crossed(at);
        let blend = Layer {
            flip: if stretched { crossed } else { blend.flip },
            ..blend
        };
        let extra = cell
            .extra_blend
            .filter(|_| self.game.three_way_blends)
            .map(|extra| {
                let extra = &self.terrain.blends[usize::from(extra)];
                let over = self.tile_uv(at, extra.tile);
                let pattern = self.pattern(extra);
                Layer {
                    uv: over.uv,
                    alpha: pattern.alpha,
                    flip: pattern.flip || (over.stretched && crossed),
                }
            });
        CellDraw { tile, blend, extra }
    }

    /// Whether the cell's diagonal from corner 0 to 2 climbs more than the one from 1 to 3, which
    /// a stretched cell splits on.
    fn crossed(&self, [x, y]: [usize; 2]) -> bool {
        let [h0, h1, h2, h3] = self.corners([x, y]);
        (h0 - h2).abs() > (h1 - h3).abs()
    }

    /// The heights of the cell's corners, in steps.
    fn corners(&self, [x, y]: [usize; 2]) -> [i32; 4] {
        [[x, y], [x + 1, y], [x + 1, y + 1], [x, y + 1]].map(|at| self.height(at))
    }

    /// The package's index of the game's sample `[x, y]`.
    const fn at(&self, [x, y]: [usize; 2]) -> usize {
        (self.rows - 1 - y) * self.columns + x
    }

    fn terrain_cell(&self, at: [usize; 2]) -> &TerrainCell {
        &self.terrain.cells[self.at(at)]
    }

    /// The class whose tiles hold the source tile `base`: the first by `first_tile` and its
    /// count.
    fn class_of(&self, base: usize) -> Option<usize> {
        self.terrain.classes.iter().position(|class| {
            (class.first_tile as usize..(class.first_tile + class.tiles) as usize).contains(&base)
        })
    }

    /// The blend's alpha at each corner and its flip, as the game sets them by its direction:
    /// opaque at the corners the blended tile covers, and a split along the edge between, so a
    /// diagonal's alpha runs along it. Without third blends, the game clears every flip a third
    /// blend asked of a blend across or along the cell.
    fn pattern(&self, blend: &BlendTile) -> Pattern {
        if blend.custom_edge.is_some() {
            return Pattern {
                alpha: [0; 4],
                flip: false,
            };
        }
        let inverted = blend.inverted;
        let flipped = blend.flipped && self.game.three_way_blends;
        let (opaque, flip): (&[usize], bool) = match blend.shape {
            BlendShape::Horizontal if inverted => (&[0, 3], flipped),
            BlendShape::Horizontal => (&[1, 2], flipped),
            BlendShape::Vertical if inverted => (&[0, 1], flipped),
            BlendShape::Vertical => (&[2, 3], flipped),
            BlendShape::RightDiagonal { long: false } if inverted => (&[1], false),
            BlendShape::RightDiagonal { long: true } if inverted => (&[0, 1, 2], false),
            BlendShape::RightDiagonal { long: false } => (&[2], true),
            BlendShape::RightDiagonal { long: true } => (&[1, 2, 3], true),
            BlendShape::LeftDiagonal { long: false } if inverted => (&[0], true),
            BlendShape::LeftDiagonal { long: true } if inverted => (&[0, 1, 3], true),
            BlendShape::LeftDiagonal { long: false } => (&[3], false),
            BlendShape::LeftDiagonal { long: true } => (&[0, 2, 3], false),
        };
        let mut alpha = [0; 4];
        for &corner in opaque {
            alpha[corner] = OPAQUE;
        }
        Pattern { alpha, flip }
    }

    /// The coordinates of `tile` over the cell `at`, as `getUVForTileIndex` gives them: its
    /// quarter of its source tile, the low bit the east half and the next the north; or, with
    /// cliffs adjusted, the cell's cliff UVs from its class's block when they name a tile of the
    /// same class, or the tile stretched up a steep cell. A tile with no texture has none.
    #[expect(
        clippy::cast_precision_loss,
        reason = "texels and steps of height lie far below 2²⁴, exact in an f32"
    )]
    fn tile_uv(&self, at: [usize; 2], tile: u16) -> TileUv {
        let base = usize::from(tile >> 2);
        let Some([x, y]) = self.atlas.tile(base) else {
            return TileUv {
                uv: [[0.0; 2]; 4],
                stretched: false,
            };
        };
        let half = (TerrainAtlas::TILE / 2) as f32;
        let (x, y) = (x as f32, y as f32);
        let n_u = if tile & 1 == 0 { x } else { x + half };
        let n_v = if tile & 2 == 0 { y + half } else { y };
        let (x_u, x_v) = (n_u + half, n_v + half);
        let square = [[n_u, x_v], [x_u, x_v], [x_u, n_v], [n_u, n_v]];
        let plain = TileUv {
            uv: square,
            stretched: false,
        };
        if !self.game.adjust_cliff_textures {
            return plain;
        }
        let class = self.class_of(base);
        if let Some(cliff) = self.terrain_cell(at).cliff_uv {
            let cliff = &self.terrain.cliffs[usize::from(cliff)];
            let matched =
                class.filter(|&class| self.class_of(usize::from(cliff.tile >> 2)) == Some(class));
            if let Some(class) = matched {
                let [left, top] = self.atlas.class(class);
                let side = self.terrain.classes[class].width * TerrainAtlas::TILE;
                let bottom = (top + side) as f32;
                let width = TerrainAtlas::WIDTH as f32;
                return TileUv {
                    uv: cliff
                        .corners
                        .map(|[u, v]| [u * width + left as f32, v * width + bottom]),
                    stretched: cliff.flip,
                };
            }
        }
        let Some(class) = class else {
            return plain;
        };
        self.stretched(at, class, square).unwrap_or(plain)
    }

    /// The square `square` of a tile of `class` stretched up the steep cell `at`, as the game's
    /// old adjustment does; none for a cell it leaves square.
    #[expect(
        clippy::cast_precision_loss,
        reason = "texels and steps of height lie far below 2²⁴, exact in an f32"
    )]
    fn stretched(&self, at: [usize; 2], class: usize, square: [[f32; 2]; 4]) -> Option<TileUv> {
        let heights = self.corners(at);
        let low = *heights.iter().min().expect("four corners");
        let high = *heights.iter().max().expect("four corners");
        let rise = high - low;
        let below_limit = low + (2 * rise + 1) / 3;
        let above_limit = low + (rise + 1) / 3;
        let below = heights.iter().filter(|&&h| h < below_limit).count();
        let above = heights.iter().filter(|&&h| h > above_limit).count();
        let slope = rise as f32 * HEIGHT_SCALE;
        if slope < STRETCH_LIMIT {
            return None;
        }
        let [left, top] = self.atlas.class(class).map(|texel| texel as f32);
        let side = (self.terrain.classes[class].width * TerrainAtlas::TILE) as f32;
        let (right, bottom) = (left + side, top + side);
        let divisor = (TILE_LIMIT / slope).clamp(1.0, TILE_LIMIT);
        let [[n_u, x_v], [x_u, _], [_, n_v], _] = square;
        let mut uv = square;
        if above != 1 && below != 1 && !(above == 2 && below == 2) && slope < DIAMOND_STRETCH_LIMIT
        {
            return None;
        }
        // One corner far from the other three: that corner's `v` moves by a share of the block.
        let lone = if below == 1 || above > below {
            Some(low)
        } else if above == 1 || below > above {
            Some(high)
        } else {
            None
        };
        if let Some(lone) = lone {
            let corner = heights
                .iter()
                .position(|&h| h == lone)
                .expect("an extreme is a corner's");
            let shift = side / divisor;
            uv[corner][1] = if corner < 2 { n_v + shift } else { x_v - shift };
        } else {
            if slope < TALL_STRETCH_LIMIT {
                return None;
            }
            let length = |rise: i32| {
                let along = rise as f32 * HEIGHT_SCALE;
                let length = (1.0 + along * along).sqrt();
                if length < STRETCH_LIMIT {
                    1.0
                } else {
                    length.min(TILE_LIMIT)
                }
            };
            let [h0, h1, h2, h3] = heights;
            let dx = length(h3 - h2) * (x_u - n_u);
            let dy = length(h3 - h0) * (x_v - n_v);
            uv = [
                [n_u, n_v + dy],
                [n_u + dx, n_v + dy],
                [n_u + dx, n_v],
                [n_u, n_v],
            ];
            let dx = length(h1 - h0) * (x_u - n_u);
            let dy = length(h2 - h1) * (x_v - n_v);
            uv[1] = [uv[0][0] + dx, uv[3][1] + dy];
        }
        // Back inside the block: down past its top, then up and left past its bottom and right.
        let up = uv.iter().map(|[_, v]| top - v).fold(0.0, f32::max);
        for [_, v] in &mut uv {
            *v += up;
        }
        let back = uv.iter().map(|[u, _]| u - right).fold(0.0, f32::max);
        let up = uv.iter().map(|[_, v]| v - bottom).fold(0.0, f32::max);
        for [u, v] in &mut uv {
            *u -= back;
            *v -= up;
        }
        Some(TileUv {
            uv,
            stretched: true,
        })
    }
}

#[cfg(test)]
mod tests;
