use std::error::Error;
use std::fmt;

use campfire_store::{AppendOpenError, DurableError};

use crate::data_dir::error::DataDirError;
use crate::error::LobbyError;
use crate::session_dir::error::{AbortError, WaitingError};

/// Why a local server does not start.
#[derive(Debug)]
pub enum LocalServerError {
    Data(DataDirError),
    /// The session an earlier start or a load left does not read back.
    Waiting(WaitingError),
    /// The session an earlier start left does not end.
    Abort(AbortError),
    /// The session's slots do not open: a bot names slot 0, the client's, or none the mode has.
    Lobby(LobbyError),
    NewSession(DurableError),
    NewJournal(AppendOpenError),
}

impl fmt::Display for LocalServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LocalServerError::Data(error) => write!(f, "{error}"),
            LocalServerError::Waiting(error) => write!(f, "{error}"),
            LocalServerError::Abort(error) => {
                write!(f, "the session an earlier start left: {error}")
            }
            LocalServerError::Lobby(error) => write!(f, "{error}"),
            LocalServerError::NewSession(error) => {
                write!(f, "could not make the session's directory: {error}")
            }
            LocalServerError::NewJournal(error) => write!(f, "the journal: {error}"),
        }
    }
}

impl Error for LocalServerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            LocalServerError::Data(error) => Some(error),
            LocalServerError::Waiting(error) => Some(error),
            LocalServerError::Abort(error) => Some(error),
            LocalServerError::Lobby(error) => Some(error),
            LocalServerError::NewSession(error) => Some(error),
            LocalServerError::NewJournal(error) => Some(error),
        }
    }
}
