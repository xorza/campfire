use std::io;
use std::net::AddrParseError;

use campfire_common::DeclaredName;
use campfire_host::ReportReadError;
use thiserror::Error;

/// Why a command line clap took names no run of the client.
#[derive(Debug, Error)]
pub(crate) enum ArgsError {
    /// A server bot in player slot 0, which the client plays unless it watches.
    #[error("player slot 0 is the client's, as it plays")]
    ClientPlayerSlot,
    /// The report's path does not resolve against the working directory.
    #[error("the report's path")]
    ReportPath(#[source] io::Error),
    /// No data directory holds the report, two folders up, and the line names none.
    #[error("no data directory holds the report, and the line names none")]
    ReportData,
    #[error("the report's facts")]
    Report(#[source] ReportReadError),
}

/// A `--watch` that names no team, as a team's name is a declared name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("no team's name")]
pub(crate) struct TeamNameError;

/// A `--watch` that names no team of the mode.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("the mode has no team {0}")]
pub(crate) struct UnknownTeam(pub(crate) DeclaredName);

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
