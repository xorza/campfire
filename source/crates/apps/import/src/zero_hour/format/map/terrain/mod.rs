use crate::zero_hour::format::map::error::TerrainError;

/// Zero Hour's terrain of one map: what each cell draws, and the tiles, blends and cliff UVs the
/// cells name, as the map's `BlendTileData` holds them, each index a cell names checked to be in
/// its list.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Terrain {
    fields: TerrainFields,
}

/// A terrain's fields, as `Terrain::new` checks them.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TerrainFields {
    /// One for each sample of the map's heights, in their order.
    pub(crate) cells: Vec<TerrainCell>,
    /// The count of source tiles, 64 texels on a side, that the texture classes cut.
    pub(crate) tiles: u32,
    /// The texture classes, each a texture `Terrain.ini` names, cut into tiles from its first.
    pub(crate) classes: Vec<TextureClass>,
    /// The count of source tiles that the edge classes cut.
    pub(crate) edge_tiles: u32,
    pub(crate) edge_classes: Vec<TextureClass>,
    pub(crate) blends: Vec<BlendTile>,
    pub(crate) cliffs: Vec<CliffUv>,
}

/// What one cell draws. A tile is a quarter of a source tile, as the original indexes them: the
/// source tile `tile >> 2`, and its quarter the low two bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerrainCell {
    pub(crate) tile: u16,
    /// The blend over the tile, an index into the blends.
    pub(crate) blend: Option<u16>,
    /// The third texture's blend, over the first, an index into the blends.
    pub(crate) extra_blend: Option<u16>,
    /// UVs that fit a cliff's tile to its slope, an index into the cliffs.
    pub(crate) cliff_uv: Option<u16>,
    /// Whether the cell is a cliff, as the map painted it.
    pub(crate) cliff: bool,
}

/// A texture of the terrain and the source tiles it is cut into: `tiles` of them from
/// `first_tile`, `width` to a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TextureClass {
    pub(crate) first_tile: u32,
    pub(crate) tiles: u32,
    pub(crate) width: u32,
    /// The class's name in `Terrain.ini`, which gives its texture.
    pub(crate) name: String,
}

/// A tile blended over a cell's own: its tile, as a cell's, and the alpha pattern that fades it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BlendTile {
    pub(crate) tile: u16,
    pub(crate) shape: BlendShape,
    /// The pattern fades the other way.
    pub(crate) inverted: bool,
    /// The cell's two triangles flip, as a third texture over a diagonal blend needs.
    pub(crate) flipped: bool,
    /// An edge class whose tile replaces the alpha pattern, an index into the edge classes.
    pub(crate) custom_edge: Option<u16>,
}

/// The direction a blend's alpha fades: across the cell, or along one of its diagonals, over a
/// corner or, `long`, over all but one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BlendShape {
    Horizontal,
    Vertical,
    RightDiagonal { long: bool },
    LeftDiagonal { long: bool },
}

/// UVs that stretch a tile over a cliff: the tile, and a `[u, v]` for each corner, upper left,
/// lower left, lower right and upper right, as the map stores them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CliffUv {
    pub(crate) tile: u16,
    pub(crate) corners: [[f32; 2]; 4],
    /// The cell's two triangles flip.
    pub(crate) flip: bool,
    /// The original's editor bent the UVs to fit.
    pub(crate) mutant: bool,
}

impl Terrain {
    /// The most tiles a side of a texture class, which the atlas's grid holds: its 2,048 texels
    /// a side of cells of a 64-texel tile and a border of 8, ⌊2,048 / 72⌋ = 28.
    pub(crate) const MOST_CLASS_WIDTH: u32 = 28;

    /// The terrain of `terrain`, its cells rows of `columns`, the heights' width; an error
    /// unless its cells fill whole rows, every index each cell, blend and cliff names is in its
    /// list, each class's tiles are within its count, and each class is no wider than the atlas
    /// holds.
    pub(crate) fn new(terrain: TerrainFields, columns: usize) -> Result<Self, TerrainError> {
        if columns == 0 || !terrain.cells.len().is_multiple_of(columns) {
            return Err(TerrainError::Shape);
        }
        let tile = |tile: u16| {
            if u32::from(tile >> 2) < terrain.tiles {
                Ok(())
            } else {
                Err(TerrainError::Tile(tile))
            }
        };
        let within = |index: Option<u16>, len: usize, error: fn(u16) -> TerrainError| match index {
            Some(index) if usize::from(index) >= len => Err(error(index)),
            _ => Ok(()),
        };
        for cell in &terrain.cells {
            tile(cell.tile)?;
            within(cell.blend, terrain.blends.len(), TerrainError::Blend)?;
            within(cell.extra_blend, terrain.blends.len(), TerrainError::Blend)?;
            within(cell.cliff_uv, terrain.cliffs.len(), TerrainError::CliffUv)?;
        }
        for blend in &terrain.blends {
            tile(blend.tile)?;
            within(
                blend.custom_edge,
                terrain.edge_classes.len(),
                TerrainError::EdgeClass,
            )?;
        }
        for cliff in &terrain.cliffs {
            tile(cliff.tile)?;
        }
        let within = |classes: &[TextureClass], tiles: u32, error: fn(usize) -> TerrainError| {
            classes.iter().enumerate().try_for_each(|(at, class)| {
                match class.first_tile.checked_add(class.tiles) {
                    Some(end) if end <= tiles => Ok(()),
                    _ => Err(error(at)),
                }
            })
        };
        within(&terrain.classes, terrain.tiles, TerrainError::Class)?;
        if let Some(at) = terrain
            .classes
            .iter()
            .position(|class| class.width > Self::MOST_CLASS_WIDTH)
        {
            return Err(TerrainError::ClassWidth(at));
        }
        within(
            &terrain.edge_classes,
            terrain.edge_tiles,
            TerrainError::EdgeClassTiles,
        )?;
        Ok(Self { fields: terrain })
    }

    pub(crate) const fn fields(&self) -> &TerrainFields {
        &self.fields
    }
}

#[cfg(test)]
mod tests;
