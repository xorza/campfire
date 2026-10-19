use std::io;
use std::process::ExitStatus;

use campfire_store::{DurableError, PathError, ReadError};
use thiserror::Error;

/// Why the vocabulary is not written.
#[derive(Debug, Error)]
pub(crate) enum VocabularyError {
    #[error("git does not start")]
    GitStart(#[source] io::Error),
    #[error("git fails with {status}: {stderr}")]
    Git { status: ExitStatus, stderr: String },
    /// A record of `git ls-files --stage` that holds no mode and path.
    #[error("git lists {0:?}, which is no file's record")]
    GitRecord(String),
    /// A path git lists that is not UTF-8, given with each byte that is not as U+FFFD.
    #[error("git lists {0:?}, which is not UTF-8")]
    NotUtf8(String),
    #[error("a Rust file does not read")]
    Read(#[source] PathError<ReadError>),
    #[error("the vocabulary does not write")]
    Write(#[source] PathError<DurableError>),
}
