use std::error::Error;
use std::fmt;
use std::io;

/// Why a durable write failed, by its step. A failed sync is never retried: the data it held may
/// be lost, and a retry can succeed over the loss.
#[derive(Debug)]
pub enum DurableError {
    /// The path names no file in a directory.
    NoName,
    Create(io::Error),
    Write(io::Error),
    Sync(io::Error),
    Rename(io::Error),
    SyncDirectory(io::Error),
}

impl fmt::Display for DurableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DurableError::NoName => f.write_str("the path names no file"),
            DurableError::Create(error) => write!(f, "could not make the temporary file: {error}"),
            DurableError::Write(error) => write!(f, "could not write the temporary file: {error}"),
            DurableError::Sync(error) => write!(f, "could not sync the temporary file: {error}"),
            DurableError::Rename(error) => {
                write!(f, "could not rename the temporary file: {error}")
            }
            DurableError::SyncDirectory(error) => {
                write!(f, "could not sync the directory: {error}")
            }
        }
    }
}

impl Error for DurableError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            DurableError::NoName => None,
            DurableError::Create(error)
            | DurableError::Write(error)
            | DurableError::Sync(error)
            | DurableError::Rename(error)
            | DurableError::SyncDirectory(error) => Some(error),
        }
    }
}
