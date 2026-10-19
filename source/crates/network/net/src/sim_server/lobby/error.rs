use campfire_common::Seat;
use campfire_mode::PickError;
use campfire_runner::TermsError;
use campfire_store::{AppendOpenError, PathError};
use campfire_visibility::VisibilityError;
use thiserror::Error;

use crate::sim_server::session_dir::error::SessionCreateError;

/// Why a session's lobby does not open.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LobbyError {
    #[error(transparent)]
    Terms(TermsError),
    /// A bot or an open seat names a seat the session does not have.
    #[error("the session has no seat {}", .0.get())]
    NoSuchSeat(Seat),
    /// Two bots, or a bot and an open seat, name the same seat.
    #[error("seat {} is a bot's or open twice", .0.get())]
    SeatNamedTwice(Seat),
    /// Every seat is a bot's or open: no player starts the session.
    #[error("every seat is a bot's or open")]
    NoPlayer,
    /// The host's visibility settings are not ones the mode leaves it.
    #[error("the host's visibility")]
    Visibility(#[source] VisibilityError),
    /// The host picks a value of no option of the mode's, or one its option lacks.
    #[error("the host's options")]
    Options(#[source] PickError),
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
