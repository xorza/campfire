use std::ffi::OsString;
use std::net::AddrParseError;
use std::num::ParseIntError;

use campfire_net::SlotBotFileError;
use thiserror::Error;

use crate::args::Flag;

/// Why the command line names nothing the server runs.
#[derive(Debug, Error)]
pub(crate) enum ArgsError {
    #[error("{}: no such flag", .0.display())]
    UnknownFlag(OsString),
    #[error("{} needs a value", .0.name())]
    NoValue(Flag),
    #[error("{} given twice", .0.name())]
    Twice(Flag),
    #[error("{}: not UTF-8", .0.display())]
    NotText(OsString),
    #[error("--data and its directory are needed")]
    NoData,
    #[error("{}: {text} is not a whole number of seconds", .flag.name())]
    Seconds {
        flag: Flag,
        text: String,
        #[source]
        error: ParseIntError,
    },
    #[error(transparent)]
    Bot(SlotBotFileError),
    /// Other than the mode and the address after the flags.
    #[error("the mode and the address are needed, and nothing after")]
    Positionals,
    #[error("{text}: not a socket address")]
    Address {
        text: String,
        #[source]
        error: AddrParseError,
    },
}
