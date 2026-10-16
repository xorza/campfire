use campfire_runner::{ResumeError, StartError};
use campfire_store::{PathError, ReadError};
use thiserror::Error;

use crate::faults::error::SnapshotWriteError;

/// Why a session a stop ended does not restore its match.
#[derive(Debug, Error)]
pub enum RestoreMatchError {
    /// The log does not start a match of the server's mode.
    #[error("the match does not start")]
    Start(#[source] StartError),
    /// The latest checkpoint's snapshot does not read.
    #[error("the latest checkpoint's snapshot does not read")]
    ReadSnapshot(#[source] PathError<ReadError>),
    /// The latest checkpoint's snapshot does not resume the match.
    #[error("the match does not resume")]
    Resume(#[source] ResumeError),
    /// A checkpoint taken again did not write its snapshot.
    #[error("a checkpoint's snapshot was not written")]
    WriteSnapshot(#[source] SnapshotWriteError),
}
