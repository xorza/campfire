use campfire_common::{BinaryError, PlayerSlot};
use thiserror::Error;

use crate::delegation::error::DelegationError;

/// Why logged bytes give no checkpoint record.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CheckpointDecodeError {
    #[error("does not decode")]
    Malformed(#[source] BinaryError),
    /// The delegation the carry holds for `slot`'s player does not parse.
    #[error("player {}'s delegation", .slot.get())]
    Delegation {
        slot: PlayerSlot,
        #[source]
        error: DelegationError,
    },
}
