use std::error::Error;
use std::fmt;

use campfire_protocol::{ConnectError, DelegationError};
use toml::de::Error as TomlError;

/// Why the server refused a player's join.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinError {
    Delegation(DelegationError),
    Connect(ConnectError),
    /// Every slot was taken by the time the join arrived.
    Full,
}

/// Why an order script does not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderScriptError {
    Toml(TomlError),
    /// A coordinate of the order at `tick` is past what a sim number holds.
    Coordinate {
        tick: u64,
    },
    /// The order at `tick` comes after an order at a later tick.
    Unordered {
        tick: u64,
    },
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

impl fmt::Display for OrderScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrderScriptError::Toml(error) => write!(f, "{error}"),
            OrderScriptError::Coordinate { tick } => {
                write!(
                    f,
                    "the order at tick {tick} has a coordinate past a sim number"
                )
            }
            OrderScriptError::Unordered { tick } => {
                write!(f, "the order at tick {tick} comes after a later one")
            }
        }
    }
}

impl Error for OrderScriptError {}
