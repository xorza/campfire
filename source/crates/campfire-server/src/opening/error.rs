use campfire_net::{AbortError, LobbyError, WaitingError};
use campfire_store::{AppendOpenError, DurableError};
use thiserror::Error;

/// Why a server did not start a session from its data directory.
#[derive(Debug, Error)]
pub(crate) enum OpeningError {
    #[error("{0}")]
    Waiting(#[source] WaitingError),
    #[error("could not end the session: {0}")]
    Abort(#[source] AbortError),
    /// The new session's slots do not open: a bot names a slot the mode does not have.
    #[error("the session does not open: {0}")]
    Lobby(#[source] LobbyError),
    /// The new session's directory or private record was not made.
    #[error("could not make the session: {0}")]
    NewSession(#[source] DurableError),
    #[error("the journal: {0}")]
    NewJournal(#[source] AppendOpenError),
}
