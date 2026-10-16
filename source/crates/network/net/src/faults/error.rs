use campfire_store::{DurableDirError, DurableError, PathError};
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

/// Why a client's receipt was not written, by its step, each of which its path's error names.
#[derive(Debug, Error)]
pub(crate) enum ReceiptWriteError {
    /// The receipts' directory was not made.
    #[error(transparent)]
    Dir(#[from] PathError<DurableDirError>),
    #[error(transparent)]
    File(#[from] PathError<DurableError>),
}
