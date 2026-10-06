use std::error::Error;
use std::fmt;

use crate::delegation::error::DelegationError;

/// Why logged bytes give no server input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerInputDecodeError {
    Malformed(postcard::Error),
    Delegation(DelegationError),
}

impl fmt::Display for ServerInputDecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServerInputDecodeError::Malformed(error) => write!(f, "does not decode: {error}"),
            ServerInputDecodeError::Delegation(error) => write!(f, "delegation: {error}"),
        }
    }
}

impl Error for ServerInputDecodeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ServerInputDecodeError::Malformed(error) => Some(error),
            ServerInputDecodeError::Delegation(error) => Some(error),
        }
    }
}
