use campfire_protocol::{ConnectError, DelegationError};
use campfire_runner::ServerInputRefused;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Why the server refused a player's join.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum JoinError {
    #[error(transparent)]
    Delegation(DelegationError),
    #[error(transparent)]
    Connect(ConnectError),
    /// Every slot was taken by the time the join arrived.
    #[error("every slot is taken")]
    Full,
    /// After the start, no slot is the player's, open to them or a bot's they may take.
    #[error("no slot is the player's or open to them")]
    NoSlot,
    /// The log refused what the join would change.
    #[error(transparent)]
    Refused(ServerInputRefused),
    /// A newer login of the player took the link's seat.
    #[error("a newer login of the player took the seat")]
    Superseded,
    /// Every spectator's place the session admits is taken.
    #[error("every spectator's place is taken")]
    NoSpectatorPlace,
}
