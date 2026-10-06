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
    #[error("{0}")]
    Terms(#[source] TermsError),
    /// A later offer names another session than the one the player plays.
    #[error("the server offers another session than the player's")]
    OtherSession,
}

/// Why a client refused a receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ReceiptRefusal {
    /// The client plays no match.
    #[error("the client plays no match")]
    NotPlaying,
    /// The server key did not sign it.
    #[error("the server key did not sign it")]
    BadSignature,
    /// It names another session, slot or delegation than the player's.
    #[error("it names another session, slot or delegation")]
    Other,
    /// It names a seq the client's history does not hold, or a head that is not the player's.
    #[error("it names a head that is not the player's at its seq")]
    OtherHead,
    /// It names an earlier seq than the one the client keeps.
    #[error("it names an earlier seq than the kept receipt")]
    Older,
}
