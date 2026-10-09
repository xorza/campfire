use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::zero_hour::error::TerrainError;

/// Zero Hour's terrain of one map, `client/maps/<name>/terrain.bin` in postcard: what each cell
/// draws, and the tiles, blends and cliff UVs the cells name, as the map's `BlendTileData` holds
/// them. Each index a cell names is checked to be in its list.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Terrain(TerrainParts);

/// A terrain's fields, as `Terrain::new` checks them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerrainParts {
    pub columns: u32,
    /// One for each sample of the map's heights, in their order.
    pub cells: Vec<TerrainCell>,
    /// The count of source tiles, 64 texels on a side, that the texture classes cut.
    pub tiles: u32,
    /// The texture classes, each a texture `Terrain.ini` names, cut into tiles from its first.
    pub classes: Vec<TextureClass>,
    /// The count of source tiles that the edge classes cut.
    pub edge_tiles: u32,
    pub edge_classes: Vec<TextureClass>,
    pub blends: Vec<BlendTile>,
    pub cliffs: Vec<CliffUv>,
}

/// What one cell draws. A tile is a quarter of a source tile, as the original indexes them: the
/// source tile `tile >> 2`, and its quarter the low two bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerrainCell {
    pub tile: u16,
    /// The blend over the tile, an index into the blends.
    pub blend: Option<u16>,
    /// The third texture's blend, over the first, an index into the blends.
    pub extra_blend: Option<u16>,
    /// UVs that fit a cliff's tile to its slope, an index into the cliffs.
    pub cliff_uv: Option<u16>,
    /// Whether the cell is a cliff, as the map painted it.
    pub cliff: bool,
}

/// A texture of the terrain and the source tiles it is cut into: `tiles` of them from
/// `first_tile`, `width` to a row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextureClass {
    pub first_tile: u32,
    pub tiles: u32,
    pub width: u32,
    /// The class's name in `Terrain.ini`.
    pub name: String,
}

/// A tile blended over a cell's own: its tile, as a cell's, and the alpha pattern that fades it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlendTile {
    pub tile: u16,
    pub shape: BlendShape,
    /// The pattern fades the other way.
    pub inverted: bool,
    /// The cell's two triangles flip, as a third texture over a diagonal blend needs.
    pub flipped: bool,
    /// An edge class whose tile replaces the alpha pattern, an index into the edge classes.
    pub custom_edge: Option<u16>,
}

/// The direction a blend's alpha fades: across the cell, or along one of its diagonals, over a
/// corner or, `long`, over all but one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlendShape {
    Horizontal,
    Vertical,
    RightDiagonal { long: bool },
    LeftDiagonal { long: bool },
}

/// UVs that stretch a tile over a cliff: the tile, and a `[u, v]` for each corner, upper left,
/// lower left, lower right and upper right, as the map stores them.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CliffUv {
    pub tile: u16,
    pub corners: [[f32; 2]; 4],
    /// The cell's two triangles flip.
    pub flip: bool,
    /// The original's editor bent the UVs to fit.
    pub mutant: bool,
}

impl Terrain {
    /// The terrain of `parts`; an error unless its cells fill whole rows, every index each
    /// cell, blend and cliff names is in its list, and each class's tiles are within its count.
    pub fn new(parts: TerrainParts) -> Result<Terrain, TerrainError> {
        let columns = usize::try_from(parts.columns)
            .ok()
            .ok_or(TerrainError::Shape)?;
        if columns == 0 || !parts.cells.len().is_multiple_of(columns) {
            return Err(TerrainError::Shape);
        }
        let tile = |tile: u16| {
            if u32::from(tile >> 2) < parts.tiles {
                Ok(())
            } else {
                Err(TerrainError::Tile(tile))
            }
        };
        let within = |index: Option<u16>, len: usize, error: fn(u16) -> TerrainError| match index {
            Some(index) if usize::from(index) >= len => Err(error(index)),
            _ => Ok(()),
        };
        for cell in &parts.cells {
            tile(cell.tile)?;
            within(cell.blend, parts.blends.len(), TerrainError::Blend)?;
            within(cell.extra_blend, parts.blends.len(), TerrainError::Blend)?;
            within(cell.cliff_uv, parts.cliffs.len(), TerrainError::CliffUv)?;
        }
        for blend in &parts.blends {
            tile(blend.tile)?;
            within(
                blend.custom_edge,
                parts.edge_classes.len(),
                TerrainError::EdgeClass,
            )?;
        }
        for cliff in &parts.cliffs {
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
        within(&parts.classes, parts.tiles, TerrainError::Class)?;
        within(
            &parts.edge_classes,
            parts.edge_tiles,
            TerrainError::EdgeClassTiles,
        )?;
        Ok(Terrain(parts))
    }

    /// The bytes of `terrain.bin`.
    pub fn encode(&self) -> Vec<u8> {
        postcard::to_stdvec(self).expect("a terrain encodes")
    }

    /// The terrain `bytes` hold; an error for bytes that hold no terrain `new` takes.
    pub fn decode(bytes: &[u8]) -> Result<Terrain, postcard::Error> {
        postcard::from_bytes(bytes)
    }

    pub const fn parts(&self) -> &TerrainParts {
        &self.0
    }
}

/// A package's file is untrusted, so a terrain `new` refuses fails to decode.
impl<'de> Deserialize<'de> for Terrain {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Terrain, D::Error> {
        Terrain::new(TerrainParts::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_terrain_decodes_only_with_each_index_in_its_list() {
        let cell = TerrainCell {
            tile: 7,
            blend: Some(0),
            extra_blend: None,
            cliff_uv: Some(0),
            cliff: true,
        };
        let parts = TerrainParts {
            columns: 2,
            cells: vec![cell; 4],
            tiles: 2,
            classes: vec![TextureClass {
                first_tile: 0,
                tiles: 2,
                width: 1,
                name: "Sand".to_owned(),
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
        };
        let terrain = Terrain::new(parts.clone()).unwrap();
        assert_eq!(Terrain::decode(&terrain.encode()).unwrap(), terrain);

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
        let mut short = parts;
        short.tiles = 1;
        let encoded = postcard::to_stdvec(&short).unwrap();
        assert!(Terrain::decode(&encoded).is_err());
    }
}
