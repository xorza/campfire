use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Why a client refused a receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum ReceiptRefusal {
    /// The client plays no match, nor left one whose link is still open.
    #[error("the client plays no match, nor left one whose link is still open")]
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
