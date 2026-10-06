use campfire_store::{AppendOpenError, DataDirError, DurableError};
use thiserror::Error;

use crate::sim_server::key_file::error::KeyFileError;
use crate::sim_server::lobby::error::LobbyError;
use crate::sim_server::session_dir::error::{AbortError, WaitingError};

/// Why a local server does not start.
#[derive(Debug, Error)]
pub enum LocalServerError {
    #[error(transparent)]
    Data(DataDirError),
    /// The server's key file does not read, or a new key is not written.
    #[error("server.nsec")]
    Key(#[source] KeyFileError),
    /// The session an earlier start or a load left does not read back.
    #[error(transparent)]
    Waiting(WaitingError),
    /// The session an earlier start left does not end.
    #[error("the session an earlier start left")]
    Abort(#[source] AbortError),
    /// The session's slots do not open: a bot names slot 0, the client's, or none the mode has.
    #[error(transparent)]
    Lobby(LobbyError),
    #[error("could not make the session's directory")]
    NewSession(#[source] DurableError),
    #[error("the journal")]
    NewJournal(#[source] AppendOpenError),
}
