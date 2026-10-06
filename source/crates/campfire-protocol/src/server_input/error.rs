use thiserror::Error;

use crate::delegation::error::DelegationError;

/// Why logged bytes give no server input.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ServerInputDecodeError {
    #[error("does not decode: {0}")]
    Malformed(#[source] postcard::Error),
    #[error("delegation: {0}")]
    Delegation(#[source] DelegationError),
}
