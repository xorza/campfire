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
