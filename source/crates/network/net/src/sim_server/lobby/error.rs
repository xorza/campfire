use campfire_common::PlayerSlot;
use campfire_mode::{PickError, PlayerSlotPickError};
use campfire_runner::TermsError;
use campfire_store::{AppendOpenError, PathError};
use campfire_visibility::VisibilityError;
use thiserror::Error;

use campfire_host::SessionCreateError;

/// Why a session's lobby does not open.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LobbyError {
    #[error(transparent)]
    Terms(TermsError),
    /// A bot or an open player slot names a player slot the session does not have.
    #[error("the session has no player slot {}", .0.get())]
    NoSuchPlayerSlot(PlayerSlot),
    /// Two bots, or a bot and an open player slot, name the same player slot.
    #[error("player slot {} is a bot's or open twice", .0.get())]
    PlayerSlotNamedTwice(PlayerSlot),
    /// Every player slot is a bot's or open, in a lobby that waits for its players: no player
    /// starts the session.
    #[error("every player slot is a bot's or open")]
    NoPlayer,
    /// The host's visibility settings are not ones the mode leaves it.
    #[error("the host's visibility")]
    Visibility(#[source] VisibilityError),
    /// The host picks a value of no option of the mode's, or one its option lacks.
    #[error("the host's options")]
    Options(#[source] PickError),
    /// The host picks a player slot's value of no choice of the mode's.
    #[error("the host's player slot picks")]
    PlayerSlotPicks(#[source] PlayerSlotPickError),
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
    Dir(#[source] SessionCreateError),
    #[error("the journal")]
    Journal(#[source] PathError<AppendOpenError>),
}
