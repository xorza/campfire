use std::io;

use campfire_protocol::LogError;
use campfire_store::{PathError, ReadError};
use thiserror::Error;

/// Why a session log is not shown: its file does not read, or decode, or the text does not
/// write.
#[derive(Debug, Error)]
pub enum ShowError {
    #[error("the log does not read")]
    Read(#[source] PathError<ReadError>),
    #[error("the log does not decode")]
    Decode(#[source] LogError),
    #[error("the text does not write")]
    Write(#[source] io::Error),
}
