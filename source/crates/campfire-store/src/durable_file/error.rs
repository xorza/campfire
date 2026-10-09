use std::io;

use thiserror::Error;

/// Why a durable write failed, by its step. A failed sync is never retried: the data it held may
/// be lost, and a retry can succeed over the loss.
#[derive(Debug, Error)]
pub enum DurableError {
    /// The path names no file in a directory.
    #[error("the path names no file")]
    NoName,
    /// The temporary file a crash left was not removed.
    #[error("could not remove a stale temporary file")]
    RemoveStale(#[source] io::Error),
    #[error("could not make the temporary file")]
    Create(#[source] io::Error),
    #[error("could not write the temporary file")]
    Write(#[source] io::Error),
    #[error("could not sync the temporary file")]
    Sync(#[source] io::Error),
    #[error("could not rename the temporary file")]
    Rename(#[source] io::Error),
    /// The temporary file of a create that found its name taken was not removed.
    #[error("could not remove the temporary file")]
    RemoveTemporary(#[source] io::Error),
    /// A directory was not removed.
    #[error("could not remove the directory")]
    Remove(#[source] io::Error),
    #[error("could not sync the directory")]
    SyncDirectory(#[source] io::Error),
}

/// Why a durable file was not made.
#[derive(Debug, Error)]
pub enum DurableCreateError {
    /// A file already holds its name, and is left as it is.
    #[error("a file already holds the name")]
    Exists,
    #[error("could not make the file")]
    Write(#[from] DurableError),
}
