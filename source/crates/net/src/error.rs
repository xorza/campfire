use std::error::Error;
use std::fmt;

use campfire_protocol::{ConnectError, DelegationError};

/// Why the server refused a player's join.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinError {
    Delegation(DelegationError),
    Connect(ConnectError),
    /// Every slot was taken by the time the join arrived.
    Full,
}

/// Why a client refused the server's offer: its terms name a session the client cannot play, or
/// another server than the one the player meant to reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TermsMismatch {
    OtherServer,
    OtherRelease,
    OtherMode,
    OtherDependencies,
    OtherTickRate,
}

impl fmt::Display for JoinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JoinError::Delegation(error) => write!(f, "{error}"),
            JoinError::Connect(error) => write!(f, "{error}"),
            JoinError::Full => f.write_str("every slot is taken"),
        }
    }
}

impl Error for JoinError {}

impl fmt::Display for TermsMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            TermsMismatch::OtherServer => "the server's key is not the one given",
            TermsMismatch::OtherRelease => "the server runs another engine release",
            TermsMismatch::OtherMode => "the server runs another mode package",
            TermsMismatch::OtherDependencies => "the server runs other dependency packages",
            TermsMismatch::OtherTickRate => "the server runs another tick rate",
        })
    }
}

impl Error for TermsMismatch {}
