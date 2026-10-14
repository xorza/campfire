use campfire_common::MapName;
use campfire_net::{AbortError, KeepFilesError, LobbyError, WaitingError};
use thiserror::Error;

/// Why a server did not start a session from its data directory.
#[derive(Debug, Error)]
pub(crate) enum OpeningError {
    #[error(transparent)]
    Waiting(WaitingError),
    /// A session waits to restore on one map, and the server was asked for another.
    #[error("a session of map {journal} waits to restore, and map {asked} was asked")]
    OtherMap { asked: MapName, journal: MapName },
    #[error("could not end the session")]
    Abort(#[source] AbortError),
    /// The new session's slots do not open: a bot names a slot the mode does not have.
    #[error("the session does not open")]
    Lobby(#[source] LobbyError),
    #[error("could not make the session")]
    KeepFiles(#[source] KeepFilesError),
}
