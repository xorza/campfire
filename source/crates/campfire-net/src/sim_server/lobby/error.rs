use campfire_common::PlayerSlot;
use campfire_runner::TermsError;
use thiserror::Error;

/// Why a session's lobby does not open.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LobbyError {
    #[error("{0}")]
    Terms(#[source] TermsError),
    /// A bot or an open slot names a slot the session does not have.
    #[error("the session has no slot {}", .0.get())]
    NoSuchSlot(PlayerSlot),
    /// Two bots, or a bot and an open slot, name the same slot.
    #[error("slot {} is a bot's or open twice", .0.get())]
    SlotNamedTwice(PlayerSlot),
    /// Every slot is a bot's or open: no player starts the session.
    #[error("every slot is a bot's or open")]
    NoPlayer,
}
