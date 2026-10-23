use std::io;

use thiserror::Error;

/// Why a durable directory was not made or removed, by its step. A failed sync is never retried,
/// as `DurableError` says.
#[derive(Debug, Error)]
pub enum DurableDirError {
    /// The path names no directory in a parent.
    #[error("the path names no directory in a parent")]
    NoParent,
    #[error("could not make the directory")]
    Create(#[source] io::Error),
    /// A link holds the directory's name, and is not followed, as a read follows none.
    #[error("a link holds the directory's name")]
    Link,
    #[error("could not remove the directory")]
    Remove(#[source] io::Error),
    #[error("could not rename the directory")]
    Rename(#[source] io::Error),
    #[error("could not sync the parent directory")]
    SyncParent(#[source] io::Error),
}

/// Why a new durable directory was not made.
#[derive(Debug, Error)]
pub enum DurableDirCreateError {
    /// A file or a directory already holds its name, and is left as it is.
    #[error("a file or a directory already holds the name")]
    Exists,
    #[error("could not make the directory")]
    Make(#[from] DurableDirError),
}
