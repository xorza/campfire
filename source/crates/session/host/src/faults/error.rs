use campfire_store::{DurableDirError, DurableError, PathError, ReadError};
use thiserror::Error;

/// Why a checkpoint's snapshot was not written, by its step, each of which its path's error
/// names.
#[derive(Debug, Error)]
pub enum SnapshotWriteError {
    /// The snapshots' directory was not made.
    #[error(transparent)]
    Dir(#[from] PathError<DurableDirError>),
    #[error(transparent)]
    File(#[from] PathError<DurableError>),
}

/// Why the snapshot of the save a load goes back to does not load: the server's own file, which
/// only a fault of its disk changes.
#[derive(Debug, Error)]
pub enum SnapshotReadError {
    #[error(transparent)]
    Read(#[from] PathError<ReadError>),
    /// It is not the one the save's checkpoint names.
    #[error("the snapshot is not the one its save names")]
    Fingerprint,
}

/// Why a client's receipt was not written, by its step, each of which its path's error names.
#[derive(Debug, Error)]
pub enum ReceiptWriteError {
    /// The receipts' directory was not made.
    #[error(transparent)]
    Dir(#[from] PathError<DurableDirError>),
    #[error(transparent)]
    File(#[from] PathError<DurableError>),
}
