use campfire_bots::BotScriptError;
use campfire_store::DataDirError;
use thiserror::Error;

use crate::key_file::error::KeyFileError;
use crate::sim_server::lobby::error::LobbyOpenError;
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
    /// The session does not open, as a bot names seat 0, the client's, or none the mode has, or
    /// it keeps no files.
    #[error(transparent)]
    Open(LobbyOpenError),
    /// A bot's script names what the match does not have.
    #[error("a bot's script")]
    Bots(#[source] BotScriptError),
}
