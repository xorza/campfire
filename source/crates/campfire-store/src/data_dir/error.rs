use std::io;

use thiserror::Error;

/// Why a data directory did not open.
#[derive(Debug, Error)]
pub enum DataDirError {
    #[error("could not make the data directory: {0}")]
    Create(#[source] io::Error),
    #[error("could not lock the data directory: {0}")]
    Lock(#[source] io::Error),
    /// Another holder, a process or this one, holds its lock.
    #[error("another process holds the data directory")]
    Locked,
}
