//! Importers: each reads a game's install as that game reads it, and writes a package of it, the
//! same bytes on every machine for one install and one importer release. Zero Hour's is the first,
//! as design 12 says.

mod gltf;
mod texture;
mod zero_hour;

pub use crate::texture::error::TextureError;
pub use crate::zero_hour::combat_import::{DroppedWeapon, WeaponDrop};
pub use crate::zero_hour::error::{
    ArchiveError, Chunk, CsfError, IniError, MapError, ModelError, NameClash, Packing,
    RefPackError, Short, TerrainError, TypeError, VersionDifference, W3dError, ZeroHourError,
};
pub use crate::zero_hour::game_version::{ArchiveHash, GameVersion};
pub use crate::zero_hour::production_import::{DropCause, DroppedButton};
pub use crate::zero_hour::{Imported, ImportedMap, SkipReason, SkippedModel, ZeroHour};
