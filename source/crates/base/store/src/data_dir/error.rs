use std::io;
use std::path::PathBuf;

use thiserror::Error;

use crate::durable_dir::error::DurableDirError;
use crate::path_error::PathError;
use campfire_platform::Exposure;

/// Why a data directory did not open, each case with the path of its step: the directory or
/// one above it, the directory, or its lock file.
#[derive(Debug, Error)]
pub enum DataDirError {
    /// It, or a directory above it, was not made.
    #[error("could not make the data directory")]
    Create(#[source] PathError<DurableDirError>),
    /// Who may open it did not read.
    #[error("could not check who may open the data directory")]
    Inspect(#[source] PathError<io::Error>),
    /// Others than its user may open it, as the OS names them.
    #[error("others may open the data directory {} ({exposure}); make it its user's only", .path.display())]
    Exposed { path: PathBuf, exposure: Exposure },
    #[error("could not lock the data directory")]
    Lock(#[source] PathError<io::Error>),
    /// Another holder, a process or this one, holds its lock.
    #[error("another process holds the data directory {}", .path.display())]
    Locked { path: PathBuf },
}
