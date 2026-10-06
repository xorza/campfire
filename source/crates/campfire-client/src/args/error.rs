use thiserror::Error;

/// Why a command line clap read names nothing the client runs.
#[derive(Debug, Error)]
pub(crate) enum ArgsError {
    /// A server bot of slot 0, which the client plays.
    #[error("slot 0 is the client's")]
    BotInClientSlot,
}
