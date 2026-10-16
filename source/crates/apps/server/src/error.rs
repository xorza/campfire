use std::io;
use std::path::PathBuf;

use campfire_bots::{OrderScriptReadError, UnknownUnitType};
use campfire_common::ExitStatus;
use campfire_net::{KeyFileError, LobbyOpenError};
use campfire_package::LoadError;
use campfire_quic::BindError;
use campfire_store::DataDirError;
use thiserror::Error;

use crate::opening::error::OpeningError;

/// Why the server's start ends before the server runs: each of its refusals, which the server
/// logs in one place, and its exit code.
#[derive(Debug, Error)]
pub(crate) enum StartError {
    /// The command line asked for help, or does not read; its parser printed why.
    #[error("the command line ends the start")]
    CommandLine(ExitStatus),
    #[error("the stop's signals are not caught")]
    Signals(#[from] io::Error),
    #[error("a bot's orders do not read")]
    BotOrders(#[from] OrderScriptReadError),
    #[error("the data directory does not open")]
    DataDir(#[from] DataDirError),
    #[error("the server's key does not open")]
    Key(#[from] KeyFileError),
    #[error("the server's socket does not open")]
    Socket(#[from] BindError),
    /// No session starts in the data directory `data`.
    #[error("no session starts in {}", data.display())]
    Session {
        data: PathBuf,
        #[source]
        error: OpeningError,
    },
    #[error("the mode does not load")]
    Mode(#[from] LoadError),
    #[error("a bot's orders name a unit type the mode does not have")]
    BotTypes(#[from] UnknownUnitType),
}

impl StartError {
    /// The exit code it ends the start with: a usage error's for a bot's orders, a storage
    /// error's for a journal that keeps no files, a failure's for any other.
    pub(crate) const fn status(&self) -> ExitStatus {
        match self {
            Self::CommandLine(status) => *status,
            Self::BotOrders(_) | Self::BotTypes(_) => ExitStatus::Usage,
            Self::Session {
                error: OpeningError::Open(LobbyOpenError::KeepFiles(_)),
                ..
            } => ExitStatus::Storage,
            Self::Signals(_)
            | Self::DataDir(_)
            | Self::Key(_)
            | Self::Socket(_)
            | Self::Session { .. }
            | Self::Mode(_) => ExitStatus::Failure,
        }
    }
}
