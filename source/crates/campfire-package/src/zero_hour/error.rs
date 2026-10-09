use thiserror::Error;

/// Why a Zero Hour terrain is not one its client draws: a count or an index that reaches past
/// what it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum TerrainError {
    /// Its cells fill no whole rows of its columns.
    #[error("its cells fill no whole rows")]
    Shape,
    /// A cell, a blend or a cliff's UVs name a tile past its tiles.
    #[error("tile {0} is past its tiles")]
    Tile(u16),
    /// A cell names a blend past its blends.
    #[error("blend {0} is past its blends")]
    Blend(u16),
    /// A cell names cliff UVs past its list.
    #[error("cliff UVs {0} are past its list")]
    CliffUv(u16),
    /// A texture class, by its index, reaches past its tiles.
    #[error("texture class {0} reaches past its tiles")]
    Class(usize),
    /// An edge class, by its index, reaches past its edge tiles.
    #[error("edge class {0} reaches past its edge tiles")]
    EdgeClassTiles(usize),
    /// A blend names an edge class past its edge classes.
    #[error("edge class {0} is past its edge classes")]
    EdgeClass(u16),
}
