use std::error::Error;
use std::fmt;
use std::io;

/// Why a data directory did not open.
#[derive(Debug)]
pub enum DataDirError {
    Create(io::Error),
    Lock(io::Error),
    /// Another holder, a process or this one, holds its lock.
    Locked,
}

impl fmt::Display for DataDirError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataDirError::Create(error) => write!(f, "could not make the data directory: {error}"),
            DataDirError::Lock(error) => write!(f, "could not lock the data directory: {error}"),
            DataDirError::Locked => f.write_str("another process holds the data directory"),
        }
    }
}

impl Error for DataDirError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            DataDirError::Create(error) | DataDirError::Lock(error) => Some(error),
            DataDirError::Locked => None,
        }
    }
}
