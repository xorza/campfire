use campfire_package::WriteError;
use campfire_store::{PathError, ReadError};
use thiserror::Error;

use crate::zero_hour::error::ZeroHourError;

/// Why a game's install does not import.
#[derive(Debug, Error)]
pub enum ImportError {
    /// A file or a directory of the install does not read.
    #[error("the install does not read")]
    Read(#[source] PathError<ReadError>),
    /// Zero Hour's install is not one the importer reads.
    #[error(transparent)]
    ZeroHour(ZeroHourError),
    /// The package does not write.
    #[error("the package does not write")]
    Write(#[source] WriteError),
}
