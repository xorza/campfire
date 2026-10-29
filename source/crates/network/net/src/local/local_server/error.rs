use campfire_common::Tick;
use campfire_host::KeyFileError;
use campfire_host::ServerBotsError;
use campfire_host::{AbortError, WaitingError};
use campfire_runner::TermsError;
use campfire_store::DataDirError;
use thiserror::Error;

use crate::sim_server::lobby::error::LobbyOpenError;

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
    /// The report's log was signed by another server's key than this data directory's, which no
    /// client pinning this server takes.
    #[error("another server's key signed the report's log")]
    OtherServer,
    /// The report names a tick its log did not run.
    #[error("the report's tick {through} is not in its log, which ends before tick {end}")]
    PastLog { through: Tick, end: Tick },
    /// The report's log reveals no seed, as a log published so far or whole does.
    #[error("the report's log reveals no seed")]
    Unrevealed,
    /// The report's terms are not those the mode's rules admit, as a mode changed since may not.
    #[error("the report's terms")]
    Terms(#[source] TermsError),
}
