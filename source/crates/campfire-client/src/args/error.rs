use std::ffi::OsString;
use std::net::AddrParseError;
use std::num::ParseIntError;

use campfire_common::NotHex;
use campfire_net::SlotBotFileError;
use campfire_protocol::secp256k1;
use thiserror::Error;

use crate::args::Flag;

/// Why the command line names nothing the client runs.
#[derive(Debug, Error)]
pub(crate) enum ArgsError {
    #[error("{}: no such option", .0.display())]
    UnknownFlag(OsString),
    #[error("{} needs a value", .0.name())]
    NoValue(Flag),
    #[error("{} given twice", .0.name())]
    Twice(Flag),
    #[error("{}: not UTF-8", .0.display())]
    NotText(OsString),
    #[error("{0}")]
    Bot(#[source] SlotBotFileError),
    /// A server bot of slot 0, which the client plays.
    #[error("slot 0 is the client's")]
    BotInClientSlot,
    #[error("the mode is needed after the options")]
    NoMode,
    #[error("--local needs --data")]
    LocalWithoutData,
    #[error("--local takes the mode alone")]
    LocalExtra,
    #[error("--server-bot needs --local")]
    BotWithoutLocal,
    /// A remote server, with other than its four arguments after the mode.
    #[error("four arguments are needed after the mode")]
    RemoteArgs,
    #[error("{text}: {error}")]
    Address {
        text: String,
        #[source]
        error: AddrParseError,
    },
    #[error("{text}: {error}")]
    Certificate {
        text: String,
        #[source]
        error: NotHex,
    },
    #[error("{text}: {error}")]
    Key {
        text: String,
        #[source]
        error: secp256k1::Error,
    },
    #[error("{text}: {error}")]
    TickRate {
        text: String,
        #[source]
        error: ParseIntError,
    },
}
