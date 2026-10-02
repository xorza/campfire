use std::error::Error;
use std::fmt;

use crate::delegation::error::ScopeError;

/// Why a server refuses a joining player's connect answer. The answer comes from the player, so
/// each is an expected failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectError {
    Scope(ScopeError),
    /// The delegation expired by the server's clock.
    Expired,
    /// The delegation's session key did not sign this challenge with this server's certificate
    /// hash: a reply relayed from a connection to another server fails here.
    BadAnswer,
}

impl fmt::Display for ConnectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConnectError::Scope(error) => error.fmt(f),
            ConnectError::Expired => f.write_str("the delegation expired"),
            ConnectError::BadAnswer => {
                f.write_str("the session key did not sign this challenge and certificate hash")
            }
        }
    }
}

impl Error for ConnectError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ConnectError::Scope(error) => Some(error),
            ConnectError::Expired | ConnectError::BadAnswer => None,
        }
    }
}
