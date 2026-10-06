use std::error::Error;
use std::fmt;

use campfire_common::PlayerSlot;

use crate::delegation::error::DelegationError;

/// Why logged bytes give no checkpoint record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckpointDecodeError {
    Malformed(postcard::Error),
    /// The delegation the carry holds for `slot`'s player does not parse.
    Delegation {
        slot: PlayerSlot,
        error: DelegationError,
    },
}

impl fmt::Display for CheckpointDecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CheckpointDecodeError::Malformed(error) => write!(f, "does not decode: {error}"),
            CheckpointDecodeError::Delegation { slot, error } => {
                write!(f, "player {}'s delegation: {error}", slot.get())
            }
        }
    }
}

impl Error for CheckpointDecodeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            CheckpointDecodeError::Malformed(error) => Some(error),
            CheckpointDecodeError::Delegation { error, .. } => Some(error),
        }
    }
}
