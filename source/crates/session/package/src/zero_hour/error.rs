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
    /// Its lighting has more than three lights, or a number that is not finite.
    #[error("its lighting has more than three lights or a number not finite")]
    Lighting,
    /// A blend names an edge class past its edge classes.
    #[error("edge class {0} is past its edge classes")]
    EdgeClass(u16),
}

/// Why a terrain class's texture is none the atlas takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ClassTextureError {
    /// It starts with no KTX2 identifier.
    #[error("it is no KTX2 file")]
    NotKtx2,
    /// A field or a level runs past its end.
    #[error("it runs past its end")]
    Short,
    /// Its format, by its Vulkan number, is not `R8G8B8A8_SRGB`.
    #[error("its format {0} is not R8G8B8A8_SRGB")]
    Format(u32),
    /// It is no 2D texture of one face and no layers, or its data is supercompressed.
    #[error("it is no plain 2D texture")]
    NotPlain,
    /// It has fewer than the three levels the atlas takes.
    #[error("it has {0} levels, fewer than 3")]
    Levels(u32),
    /// A level, by its number, is not its size's bytes.
    #[error("level {0} is not its size's bytes")]
    LevelBytes(usize),
}
