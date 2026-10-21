use std::net::AddrParseError;

use thiserror::Error;

/// Why a command line clap took names no run of the client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub(crate) enum ArgsError {
    /// A server bot in slot 0, which the client plays unless it watches.
    #[error("slot 0 is the client's, as it plays")]
    ClientSlot,
}

/// A `--watch` that names no team, as a team's name is a declared name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("no team's name")]
pub(crate) struct TeamNameError;

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
