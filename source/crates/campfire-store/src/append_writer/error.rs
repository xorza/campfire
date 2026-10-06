use std::io;

use thiserror::Error;

use crate::durable_file::error::DurableError;

/// Why an append writer's file did not open.
#[derive(Debug, Error)]
pub enum AppendOpenError {
    /// The new file's first bytes were not written durably.
    #[error("could not make the file")]
    Create(#[source] DurableError),
    #[error("could not open the file")]
    Open(#[source] io::Error),
    /// The file was not cut to its whole records, or the cut not synced.
    #[error("could not cut the file's torn tail")]
    Cut(#[source] io::Error),
}

/// Why an append writer stopped, by its step. Neither is retried: the data a failed sync held
/// may be lost, and a retry can succeed over the loss.
#[derive(Debug, Error)]
pub enum AppendError {
    #[error("could not append")]
    Write(#[source] io::Error),
    #[error("could not sync what was appended")]
    Sync(#[source] io::Error),
}
