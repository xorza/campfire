use std::error::Error;
use std::fmt;
use std::io;

use campfire_protocol::{DurableError, JournalError};
use campfire_runner::TermsError;

use crate::data_dir::error::DataDirError;
use crate::session_dir::error::{AbortError, FindError, RestoreError};

/// Why a local server does not start.
#[derive(Debug)]
pub enum LocalServerError {
    Data(DataDirError),
    /// The session an earlier start left is not found.
    Find(FindError),
    /// The session an earlier start left does not read.
    Restore(RestoreError),
    /// The directory of a session whose match never started is not removed.
    Remove(io::Error),
    /// The session an earlier start left does not end.
    Abort(AbortError),
    /// The mode does not run at its default rate, or has no slot for the client.
    Terms(TermsError),
    NewSession(DurableError),
    NewJournal(JournalError),
}

impl fmt::Display for LocalServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LocalServerError::Data(error) => write!(f, "{error}"),
            LocalServerError::Find(error) => write!(f, "{error}"),
            LocalServerError::Restore(error) => {
                write!(f, "the session an earlier start left: {error}")
            }
            LocalServerError::Remove(error) => {
                write!(f, "could not remove a session that never started: {error}")
            }
            LocalServerError::Abort(error) => {
                write!(f, "the session an earlier start left: {error}")
            }
            LocalServerError::Terms(error) => write!(f, "{error}"),
            LocalServerError::NewSession(error) => {
                write!(f, "could not make the session's directory: {error}")
            }
            LocalServerError::NewJournal(error) => write!(f, "{error}"),
        }
    }
}

impl Error for LocalServerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            LocalServerError::Data(error) => Some(error),
            LocalServerError::Find(error) => Some(error),
            LocalServerError::Restore(error) => Some(error),
            LocalServerError::Remove(error) => Some(error),
            LocalServerError::Abort(error) => Some(error),
            LocalServerError::Terms(error) => Some(error),
            LocalServerError::NewSession(error) => Some(error),
            LocalServerError::NewJournal(error) => Some(error),
        }
    }
}
