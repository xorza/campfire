use std::error::Error;
use std::fmt;
use std::io;

use crate::durable_file::error::DurableError;

/// Why an append writer's file did not open.
#[derive(Debug)]
pub enum AppendOpenError {
    /// The new file's first bytes were not written durably.
    Create(DurableError),
    Open(io::Error),
    /// The file was not cut to its whole records, or the cut not synced.
    Cut(io::Error),
}

impl fmt::Display for AppendOpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppendOpenError::Create(error) => write!(f, "could not make the file: {error}"),
            AppendOpenError::Open(error) => write!(f, "could not open the file: {error}"),
            AppendOpenError::Cut(error) => write!(f, "could not cut the file's torn tail: {error}"),
        }
    }
}

impl Error for AppendOpenError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            AppendOpenError::Create(error) => Some(error),
            AppendOpenError::Open(error) | AppendOpenError::Cut(error) => Some(error),
        }
    }
}

/// Why an append writer stopped, by its step. Neither is retried: the data a failed sync held
/// may be lost, and a retry can succeed over the loss.
#[derive(Debug)]
pub enum AppendError {
    Write(io::Error),
    Sync(io::Error),
}

impl fmt::Display for AppendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppendError::Write(error) => write!(f, "could not append: {error}"),
            AppendError::Sync(error) => write!(f, "could not sync what was appended: {error}"),
        }
    }
}

impl Error for AppendError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            AppendError::Write(error) | AppendError::Sync(error) => Some(error),
        }
    }
}
