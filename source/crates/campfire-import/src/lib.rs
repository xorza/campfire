//! Importers: each reads a game's install as that game reads it, and writes a package of it, the
//! same bytes on every machine for one install and one importer release. Zero Hour's is the first,
//! as design 12 says.

mod error;
mod gltf;
mod texture;
mod zero_hour;

pub use crate::error::ImportError;
pub use crate::texture::error::TextureError;
pub use crate::zero_hour::error::{
    ArchiveError, Chunk, MapError, Packing, RefPackError, VersionDifference, ZeroHourError,
};
pub use crate::zero_hour::game_version::{ArchiveHash, GameVersion};
pub use crate::zero_hour::{Imported, SkipReason, SkippedModel, ZeroHour};

/// What other crates' tests take of the import.
#[cfg(feature = "internals")]
pub mod internals {
    use crate::texture::internals::ktx2;

    /// The import's two fixture textures as it writes them: a DDS of DXT1 blocks, 8 × 8 with its
    /// 4 levels, BC1 in KTX2; and a 32-bit TGA of 2 × 1, RGBA8 with its 2 levels.
    pub fn ktx2_fixtures() -> [Vec<u8>; 2] {
        ktx2()
    }
}
