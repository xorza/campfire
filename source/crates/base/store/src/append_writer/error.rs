use std::io;
use std::time::Duration;

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

/// Why an append writer stopped. None is retried: the data a failed sync held may be lost, and
/// a retry can succeed over the loss.
#[derive(Debug, Error)]
pub enum AppendError {
    #[error("could not append")]
    Write(#[source] io::Error),
    #[error("could not sync what was appended")]
    Sync(#[source] io::Error),
    /// A write and its sync ran for `running` and did not return.
    #[error("a write and its sync ran for {running:?} and did not return")]
    Hung { running: Duration },
    /// The records the worker did not take came to `bytes`.
    #[error("{bytes} bytes of records waited for the disk")]
    Behind { bytes: usize },
}
