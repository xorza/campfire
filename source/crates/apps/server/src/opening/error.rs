use campfire_common::MapName;
use campfire_host::{AbortError, WaitingError};
use campfire_mode::{PickError, PlayerSlotPickError};
use campfire_net::LobbyOpenError;
use campfire_runner::TermsError;
use campfire_visibility::VisibilityError;
use thiserror::Error;

/// Why a server did not start a session from its data directory.
#[derive(Debug, Error)]
pub(crate) enum OpeningError {
    #[error(transparent)]
    Waiting(WaitingError),
    /// A session waits to restore on one map, and the server was asked for another.
    #[error("a session of map {journal} waits to restore, and map {asked} was asked")]
    OtherMap { asked: MapName, journal: MapName },
    /// The options' picks asked are none of the mode's.
    #[error("the options asked")]
    Options(#[source] PickError),
    /// The player slot picks asked are none of the mode's choices.
    #[error("the player slot picks")]
    PlayerSlotPicks(#[source] PlayerSlotPickError),
    /// A session waits to restore, and other picks of its options or player slots than its own were
    /// asked.
    #[error(
        "a session waits to restore, and other picks of its options or player slots were asked"
    )]
    OtherPicks,
    /// The visibility settings asked are none the mode leaves the host.
    #[error("the visibility asked")]
    Visibility(#[source] VisibilityError),
    /// A session waits to restore, and another visibility than its own was asked.
    #[error("a session waits to restore, and another visibility or most of spectators was asked")]
    OtherVisibility,
    #[error("could not end the session")]
    Abort(#[source] AbortError),
    /// The new session does not open, as a bot names a player slot the mode does not have, or keeps
    /// no files.
    #[error("the session does not open")]
    Open(#[source] LobbyOpenError),
    /// The session's terms are not of the mode's rules, as a journal of another release holds.
    #[error("the session's terms are not the mode's")]
    Terms(#[source] TermsError),
}
