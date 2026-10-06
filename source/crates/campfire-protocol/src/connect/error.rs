use thiserror::Error;

use crate::delegation::error::ScopeError;

/// Why a server refuses a joining player's connect answer. The answer comes from the player, so
/// each is an expected failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ConnectError {
    #[error("{0}")]
    Scope(#[source] ScopeError),
    /// The delegation expired by the server's clock.
    #[error("the delegation expired")]
    Expired,
    /// The delegation's session key did not sign this challenge with this server's certificate
    /// hash: a reply relayed from a connection to another server fails here.
    #[error("the session key did not sign this challenge and certificate hash")]
    BadAnswer,
}
