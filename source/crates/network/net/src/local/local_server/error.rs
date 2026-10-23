use campfire_store::DataDirError;
use thiserror::Error;

use crate::sim_server::lobby::error::LobbyOpenError;
use campfire_host::KeyFileError;
use campfire_host::ServerBotsError;
use campfire_host::{AbortError, WaitingError};

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
    /// The session does not open, as a bot names player slot 0, the client's, or none the mode has,
    /// or it keeps no files.
    #[error(transparent)]
    Open(LobbyOpenError),
    /// A bot's order script names what the match does not have, or the mode names no bot of a
    /// bot's name.
    #[error("a bot")]
    Bots(#[source] ServerBotsError),
}
