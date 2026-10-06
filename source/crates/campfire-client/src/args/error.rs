use thiserror::Error;

/// Why the command line names nothing the client runs.
#[derive(Debug, Error)]
pub(crate) enum ArgsError {
    /// clap refuses the command line, or it asks for the help or the version, which clap writes.
    #[error(transparent)]
    CommandLine(clap::Error),
    /// A server bot of slot 0, which the client plays.
    #[error("slot 0 is the client's")]
    BotInClientSlot,
}
