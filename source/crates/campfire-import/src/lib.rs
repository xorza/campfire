//! Importers: each reads a game's install as that game reads it, and writes a package of it, the
//! same bytes on every machine for one install and one importer release. Zero Hour's is the first,
//! as design 12 says.

mod error;
mod zero_hour;

pub use crate::error::ImportError;
pub use crate::zero_hour::ZeroHour;
pub use crate::zero_hour::error::{ArchiveError, RefPackError, VersionDifference, ZeroHourError};
pub use crate::zero_hour::game_version::{ArchiveHash, GameVersion};
