use std::io;

use thiserror::Error;

use crate::durable_dir::error::DurableDirError;
use crate::path_error::PathError;
use crate::platform::owner_only::Exposure;

/// Why a data directory did not open.
#[derive(Debug, Error)]
pub enum DataDirError {
    /// It, or a directory above it, was not made.
    #[error("could not make the data directory")]
    Create(#[source] PathError<DurableDirError>),
    /// Who may open it did not read.
    #[error("could not check who may open the data directory")]
    Inspect(#[source] io::Error),
    /// Others than its owner may open it, as the OS names them.
    #[error("others may open the data directory ({0}); make it its owner's only")]
    Exposed(Exposure),
    #[error("could not lock the data directory")]
    Lock(#[source] io::Error),
    /// Another holder, a process or this one, holds its lock.
    #[error("another process holds the data directory")]
    Locked,
}
