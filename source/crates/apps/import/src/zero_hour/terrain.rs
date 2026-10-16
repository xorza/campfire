use bevy_math::UVec2;
use campfire_values::PackagePath;

use crate::zero_hour::error::TerrainError;

/// Zero Hour's terrain of one map: what each cell draws, and the tiles, blends and cliff UVs the
/// cells name, as the map's `BlendTileData` holds them, each index a cell names checked to be in
/// its list.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Terrain {
    parts: TerrainParts,
}

/// A terrain's fields, as `Terrain::new` checks them.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TerrainParts {
    pub(crate) columns: u32,
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
    /// The class's name in `Terrain.ini`.
    pub(crate) name: String,
    /// The texture `Terrain.ini` gives the class; none when it names none, or a TGA the install
    /// lacks, which the game draws as no texture.
    pub(crate) texture: Option<ClassTexture>,
}

/// A class's texture: the package's file of it, and its size in texels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClassTexture {
    pub(crate) path: PackagePath,
    pub(crate) size: UVec2,
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
    /// The terrain of `terrain`; an error unless its cells fill whole rows, every index each
    /// cell, blend and cliff names is in its list, and each class's tiles are within its count.
    pub(crate) fn new(terrain: TerrainParts) -> Result<Self, TerrainError> {
        let columns = usize::try_from(terrain.columns)
            .ok()
            .ok_or(TerrainError::Shape)?;
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
        within(
            &terrain.edge_classes,
            terrain.edge_tiles,
            TerrainError::EdgeClassTiles,
        )?;
        Ok(Self { parts: terrain })
    }

    pub(crate) const fn parts(&self) -> &TerrainParts {
        &self.parts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A terrain of 2 × 2 cells, each naming each list's one entry.
    fn parts() -> TerrainParts {
        let cell = TerrainCell {
            tile: 7,
            blend: Some(0),
            extra_blend: None,
            cliff_uv: Some(0),
            cliff: true,
        };
        TerrainParts {
            columns: 2,
            cells: vec![cell; 4],
            tiles: 2,
            classes: vec![TextureClass {
                first_tile: 0,
                tiles: 2,
                width: 1,
                name: "Sand".to_owned(),
                texture: None,
            }],
            edge_tiles: 0,
            edge_classes: Vec::new(),
            blends: vec![BlendTile {
                tile: 4,
                shape: BlendShape::LeftDiagonal { long: true },
                inverted: true,
                flipped: false,
                custom_edge: None,
            }],
            cliffs: vec![CliffUv {
                tile: 0,
                corners: [[0.0, 1.0], [0.0, 0.0], [1.5, 0.0], [1.5, 1.0]],
                flip: false,
                mutant: true,
            }],
        }
    }

    #[test]
    fn a_terrain_holds_only_with_each_index_in_its_list() {
        let parts = parts();
        assert_eq!(Terrain::new(parts.clone()).unwrap().parts(), &parts);
        // Tile 7 is source tile 1 of 2; tile 8 would be source tile 2. Each list holds one entry,
        // so index 1 is past it.
        let with = |change: fn(&mut TerrainParts)| {
            let mut parts = parts.clone();
            change(&mut parts);
            Terrain::new(parts).unwrap_err()
        };
        assert_eq!(
            with(|parts| parts.cells.pop().map(drop).unwrap()),
            TerrainError::Shape
        );
        assert_eq!(with(|parts| parts.columns = 0), TerrainError::Shape);
        assert_eq!(with(|parts| parts.cells[3].tile = 8), TerrainError::Tile(8));
        assert_eq!(
            with(|parts| parts.cells[1].blend = Some(1)),
            TerrainError::Blend(1)
        );
        assert_eq!(
            with(|parts| parts.cells[2].extra_blend = Some(1)),
            TerrainError::Blend(1)
        );
        assert_eq!(
            with(|parts| parts.cells[0].cliff_uv = Some(1)),
            TerrainError::CliffUv(1)
        );
        assert_eq!(
            with(|parts| parts.blends[0].tile = 9),
            TerrainError::Tile(9)
        );
        assert_eq!(
            with(|parts| parts.blends[0].custom_edge = Some(0)),
            TerrainError::EdgeClass(0)
        );
        assert_eq!(
            with(|parts| parts.cliffs[0].tile = 12),
            TerrainError::Tile(12)
        );
        // The class's tiles 0 and 1 are the terrain's 2; from 1 they would reach a third.
        assert_eq!(
            with(|parts| parts.classes[0].first_tile = 1),
            TerrainError::Class(0)
        );
        assert_eq!(
            with(|parts| parts.edge_classes.push(parts.classes[0].clone())),
            TerrainError::EdgeClassTiles(0)
        );
    }
}
