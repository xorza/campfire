use thiserror::Error;

use crate::durable_dir::error::{DurableDirCreateError, DurableDirError};
use crate::input_file::error::ReadError;
use crate::path_error::PathError;

/// Why a run's directory was not made.
#[derive(Debug, Error)]
pub enum RunDirError {
    /// The run root, or a directory above it, is not made.
    #[error("the run root is not made")]
    Root(#[source] PathError<DurableDirError>),
    /// The run's directory is not made, or another run's holds its name.
    #[error("the run's directory is not made")]
    Dir(#[source] PathError<DurableDirCreateError>),
    /// The run root does not list.
    #[error("the run root does not list")]
    List(#[source] PathError<ReadError>),
    /// The directory of a run before the newest it keeps does not go.
    #[error("an old run's directory does not go")]
    Prune(#[source] PathError<DurableDirError>),
}
