use std::net::AddrParseError;

use campfire_net::SlotBotFileError;
use thiserror::Error;

/// Why a `--server-bot` names no bot of the local server.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub(crate) enum ServerBotError {
    #[error(transparent)]
    File(SlotBotFileError),
    /// A bot of slot 0, which the client plays.
    #[error("slot 0 is the client's")]
    ClientSlot,
}

/// Why a server's address names no server a client can reach.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub(crate) enum ServerAddressError {
    #[error(transparent)]
    Syntax(AddrParseError),
    /// `0.0.0.0` or `::`, which a server binds and no client reaches.
    #[error("the address names no host")]
    Unspecified,
    #[error("the address names no port")]
    NoPort,
}
