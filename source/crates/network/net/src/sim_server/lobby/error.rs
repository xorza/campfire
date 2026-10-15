use campfire_common::PlayerSlot;
use campfire_runner::TermsError;
use campfire_store::{AppendOpenError, DurableError, PathError};
use campfire_visibility::VisibilityError;
use thiserror::Error;

/// Why a session's lobby does not open.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LobbyError {
    #[error(transparent)]
    Terms(TermsError),
    /// A bot or an open slot names a slot the session does not have.
    #[error("the session has no slot {}", .0.get())]
    NoSuchSlot(PlayerSlot),
    /// Two bots, or a bot and an open slot, name the same slot.
    #[error("slot {} is a bot's or open twice", .0.get())]
    SlotNamedTwice(PlayerSlot),
    /// Every slot is a bot's or open: no player starts the session.
    #[error("every slot is a bot's or open")]
    NoPlayer,
    /// The host's visibility settings are not ones the mode leaves it.
    #[error("the host's visibility")]
    Visibility(#[source] VisibilityError),
}

/// Why a server's new session does not open.
#[derive(Debug, Error)]
pub enum LobbyOpenError {
    #[error(transparent)]
    Lobby(#[from] LobbyError),
    #[error(transparent)]
    KeepFiles(#[from] KeepFilesError),
}

/// Why a lobby's session keeps no files.
#[derive(Debug, Error)]
pub enum KeepFilesError {
    /// The session's directory or private record was not made.
    #[error("could not make the session's directory")]
    Dir(#[source] PathError<DurableError>),
    #[error("the journal")]
    Journal(#[source] PathError<AppendOpenError>),
}
