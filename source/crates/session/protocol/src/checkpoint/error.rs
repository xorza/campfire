use campfire_codec::BinaryError;
use campfire_common::Seat;
use thiserror::Error;

use crate::delegation::error::DelegationError;

/// Why logged bytes give no checkpoint record.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CheckpointDecodeError {
    #[error("does not decode")]
    Malformed(#[source] BinaryError),
    /// The delegation the carry holds for `seat`'s player does not parse.
    #[error("player {}'s delegation", .seat.get())]
    Delegation {
        seat: Seat,
        #[source]
        error: DelegationError,
    },
}
