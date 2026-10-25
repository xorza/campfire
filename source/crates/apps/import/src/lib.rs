//! Importers: each reads a game's install as that game reads it, and writes a package of it, the
//! same bytes on every machine for one install and one importer release. Zero Hour's is the first
//! (09-zero-hour.md, Zero Hour is the first import).

mod gltf;
mod pose;
mod quaternion;
mod texture;
mod zero_hour;

pub use crate::texture::error::TextureError;
pub use crate::zero_hour::archive_path::ArchivePath;
pub use crate::zero_hour::dropped::Dropped;
pub use crate::zero_hour::error::{
    MapError, ModelError, NameClash, NameError, NoPackagePath, TypeError, VersionDifference,
    ZeroHourError,
};
pub use crate::zero_hour::format::big::error::ArchiveError;
pub use crate::zero_hour::format::csf::error::CsfError;
pub use crate::zero_hour::format::ini::error::{IniError, MoneyError};
pub use crate::zero_hour::format::ini::object_ini::build_variations::VariationDropCause;
pub use crate::zero_hour::format::map::error::{Chunk, MapReadError, Packing, TerrainError};
pub use crate::zero_hour::format::ref_pack::error::RefPackError;
pub use crate::zero_hour::format::w3d::error::W3dError;
pub use crate::zero_hour::game_version::GameVersion;
pub use crate::zero_hour::import_loss::ImportLoss;
pub use crate::zero_hour::model_writer::{SkipReason, SkippedModel};
pub use crate::zero_hour::production_import::ButtonDropCause;
pub use crate::zero_hour::supply_import::SupplyDropCause;
pub use crate::zero_hour::upgrade_import::UpgradeDropCause;
pub use crate::zero_hour::weapon_drop_cause::WeaponDropCause;
pub use crate::zero_hour::{Imported, ZeroHour};
