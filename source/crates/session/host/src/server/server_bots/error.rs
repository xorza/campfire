use campfire_bots::BotScriptError;
use campfire_common::DeclaredName;
use thiserror::Error;

/// Why a server's bots do not resolve in their match.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ServerBotsError {
    /// An order script names what the match does not have.
    #[error(transparent)]
    Script(#[from] BotScriptError),
    /// The mode names no bot of the name.
    #[error("the mode has no bot {0}")]
    NoBot(DeclaredName),
}
