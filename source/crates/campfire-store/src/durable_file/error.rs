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
    #[error("could not remove a stale temporary file: {0}")]
    RemoveStale(#[source] io::Error),
    #[error("could not make the temporary file: {0}")]
    Create(#[source] io::Error),
    #[error("could not write the temporary file: {0}")]
    Write(#[source] io::Error),
    #[error("could not sync the temporary file: {0}")]
    Sync(#[source] io::Error),
    #[error("could not rename the temporary file: {0}")]
    Rename(#[source] io::Error),
    /// A directory was not removed.
    #[error("could not remove the directory: {0}")]
    Remove(#[source] io::Error),
    #[error("could not sync the directory: {0}")]
    SyncDirectory(#[source] io::Error),
}
