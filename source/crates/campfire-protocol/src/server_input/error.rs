use campfire_common::BinaryError;
use thiserror::Error;

use crate::delegation::error::DelegationError;

/// Why logged bytes give no server input.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ServerInputDecodeError {
    #[error("does not decode")]
    Malformed(#[source] BinaryError),
    #[error("delegation")]
    Delegation(#[source] DelegationError),
}
