use campfire_bots::BotScriptError;
use campfire_common::ExitStatus;
use campfire_host::{KeyFileError, OrderFileError, ReportLogError};
use campfire_net::LocalServerError;
use campfire_net_socket::BindError;
use campfire_package::LoadError;
use campfire_runner::TermsError;
use campfire_store::DataDirError;
use thiserror::Error;

use crate::command_line::error::{ArgsError, UnknownTeam};

/// Why the client's start ends before its window opens: each of its refusals, which the client
/// logs in one place, and its exit code.
#[derive(Debug, Error)]
pub(crate) enum ClientLaunchError {
    /// The command line asked for help, or does not read; its parser printed why.
    #[error("the command line ends the start")]
    CommandLine(ExitStatus),
    #[error("the command line is refused")]
    Args(#[from] ArgsError),
    #[error("the mode does not load")]
    Mode(#[from] LoadError),
    #[error("a bot's orders do not read")]
    BotOrders(#[from] OrderFileError),
    #[error("the orders name what the mode does not have")]
    BotNames(#[from] BotScriptError),
    #[error("the key file does not read")]
    Key(#[from] KeyFileError),
    #[error("the mode has no such team")]
    Team(#[from] UnknownTeam),
    #[error("the data directory does not open")]
    DataDir(#[from] DataDirError),
    #[error("the report's log does not read")]
    ReportLog(#[from] ReportLogError),
    #[error("the local server does not start")]
    LocalServer(#[from] LocalServerError),
    #[error("the mode does not run at the listing's rate")]
    Rate(#[from] TermsError),
    #[error("the client's socket does not open")]
    Socket(#[from] BindError),
}

impl ClientLaunchError {
    /// The exit code it ends the start with: a usage error's for what the command line names
    /// wrong, a failure's for what does not read, open or start.
    pub(crate) const fn status(&self) -> ExitStatus {
        match self {
            Self::CommandLine(status) => *status,
            Self::Args(_) | Self::BotNames(_) | Self::Team(_) | Self::Rate(_) => ExitStatus::Usage,
            Self::Mode(_)
            | Self::BotOrders(_)
            | Self::Key(_)
            | Self::DataDir(_)
            | Self::ReportLog(_)
            | Self::LocalServer(_)
            | Self::Socket(_) => ExitStatus::Failure,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use campfire_common::DeclaredName;

    use super::*;

    #[test]
    fn a_launch_error_ends_with_its_status() {
        let team = UnknownTeam(DeclaredName::new("north").unwrap());
        let locked = DataDirError::Locked {
            path: PathBuf::from("data"),
        };
        for (error, status) in [
            (
                ClientLaunchError::CommandLine(ExitStatus::Success),
                ExitStatus::Success,
            ),
            (
                ClientLaunchError::Args(ArgsError::ClientPlayerSlot),
                ExitStatus::Usage,
            ),
            (
                ClientLaunchError::Args(ArgsError::ReportData),
                ExitStatus::Usage,
            ),
            (ClientLaunchError::Team(team), ExitStatus::Usage),
            (ClientLaunchError::DataDir(locked), ExitStatus::Failure),
        ] {
            assert_eq!(error.status(), status, "{error}");
        }
    }
}
