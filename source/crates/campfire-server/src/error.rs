use std::error::Error;
use std::fmt;
use std::io;

use campfire_net::{AbortError, FindError, RestoreError};
use campfire_protocol::{DurableError, JournalError};

/// Why a server did not start a session from its data directory.
#[derive(Debug)]
pub(crate) enum OpeningError {
    Find(FindError),
    Restore(RestoreError),
    /// The directory of a session whose match never started did not go.
    Remove(io::Error),
    Abort(AbortError),
    /// The new session's directory or private record was not made.
    NewSession(DurableError),
    NewJournal(JournalError),
}

impl fmt::Display for OpeningError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpeningError::Find(error) => write!(f, "{error}"),
            OpeningError::Restore(error) => write!(f, "the session does not restore: {error}"),
            OpeningError::Remove(error) => {
                write!(f, "could not remove a session that never started: {error}")
            }
            OpeningError::Abort(error) => write!(f, "could not end the session: {error}"),
            OpeningError::NewSession(error) => write!(f, "could not make the session: {error}"),
            OpeningError::NewJournal(error) => write!(f, "{error}"),
        }
    }
}

impl Error for OpeningError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            OpeningError::Find(error) => Some(error),
            OpeningError::Restore(error) => Some(error),
            OpeningError::Remove(error) => Some(error),
            OpeningError::Abort(error) => Some(error),
            OpeningError::NewSession(error) => Some(error),
            OpeningError::NewJournal(error) => Some(error),
        }
    }
}
