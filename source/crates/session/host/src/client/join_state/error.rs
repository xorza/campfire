use campfire_runner::TermsError;
use thiserror::Error;

/// Why a client refused the server's offer: its terms name a session the client cannot play, or
/// another server or tick rate than the listing the player reached it by.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TermsMismatch {
    #[error("the server's key is not the one given")]
    OtherServer,
    #[error("the server runs another tick rate than the one given")]
    OtherTickRate,
    #[error(transparent)]
    Terms(TermsError),
    /// A later offer names another session than the one the player plays.
    #[error("the server offers another session than the player's")]
    OtherSession,
}
