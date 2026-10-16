//! Importers: each reads a game's install as that game reads it, and writes a package of it, the
//! same bytes on every machine for one install and one importer release. Zero Hour's is the first
//! (12-zero-hour.md, Zero Hour is the first import).

mod gltf;
mod texture;
mod zero_hour;

pub use crate::texture::error::TextureError;
pub use crate::zero_hour::combat_import::DroppedWeapon;
pub use crate::zero_hour::error::{
    MapError, ModelError, NameClash, NoPackagePath, TypeError, VersionDifference, ZeroHourError,
};
pub use crate::zero_hour::format::big::error::ArchiveError;
pub use crate::zero_hour::format::csf::error::CsfError;
pub use crate::zero_hour::format::ini::error::{IniError, MoneyError};
pub use crate::zero_hour::format::map::error::{Chunk, MapReadError, Packing, TerrainError};
pub use crate::zero_hour::format::ref_pack::error::RefPackError;
pub use crate::zero_hour::format::w3d::error::W3dError;
pub use crate::zero_hour::game_version::GameVersion;
pub use crate::zero_hour::production_import::{DropCause, DroppedButton};
pub use crate::zero_hour::weapon_drop::WeaponDrop;
pub use crate::zero_hour::{Imported, ImportedMap, SkipReason, SkippedModel, ZeroHour};
