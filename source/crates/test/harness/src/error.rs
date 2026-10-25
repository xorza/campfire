use std::error::Error;
use std::fmt;

use campfire_store::{DurableDirError, DurableError, PathError};

/// Why a probe's file does not write.
#[derive(Debug)]
pub enum ProbeWriteError {
    Dir(PathError<DurableDirError>),
    File(PathError<DurableError>),
}

impl fmt::Display for ProbeWriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dir(error) => {
                let dir = error.path.display();
                write!(f, "the probe's directory {dir} does not make")
            }
            Self::File(error) => {
                let file = error.path.display();
                write!(f, "the probe's file {file} does not write")
            }
        }
    }
}

impl Error for ProbeWriteError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Dir(error) => Some(&error.error),
            Self::File(error) => Some(&error.error),
        }
    }
}

impl From<PathError<DurableDirError>> for ProbeWriteError {
    fn from(error: PathError<DurableDirError>) -> Self {
        Self::Dir(error)
    }
}

impl From<PathError<DurableError>> for ProbeWriteError {
    fn from(error: PathError<DurableError>) -> Self {
        Self::File(error)
    }
}
