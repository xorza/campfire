use std::error::Error;
use std::fmt;

use campfire_protocol::{ConnectError, DelegationError};
use campfire_runner::{ServerInputRefused, TermsError};
use toml::de::Error as TomlError;

/// Why the server refused a player's join.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinError {
    Delegation(DelegationError),
    Connect(ConnectError),
    /// Every slot was taken by the time the join arrived.
    Full,
    /// After the start, no slot is the player's, open to them or a bot's they may take.
    NoSlot,
    /// The log refused what the join would change.
    Refused(ServerInputRefused),
    /// A newer login of the player took the link's seat.
    Superseded,
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
    /// The order at `tick` names no action, or more than one.
    Action {
        tick: u64,
    },
    /// The script ends at `end`, before its last order.
    EndsEarly {
        end: u64,
    },
}

/// Why a client refused the server's offer: its terms name a session the client cannot play, or
/// another server or tick rate than the listing the player reached it by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TermsMismatch {
    OtherServer,
    OtherTickRate,
    Terms(TermsError),
    /// A later offer names another session than the one the player plays.
    OtherSession,
}

impl fmt::Display for JoinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JoinError::Delegation(error) => write!(f, "{error}"),
            JoinError::Connect(error) => write!(f, "{error}"),
            JoinError::Full => f.write_str("every slot is taken"),
            JoinError::NoSlot => f.write_str("no slot is the player's or open to them"),
            JoinError::Refused(error) => write!(f, "{error}"),
            JoinError::Superseded => f.write_str("a newer login of the player took the seat"),
        }
    }
}

impl Error for JoinError {}

impl fmt::Display for TermsMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TermsMismatch::OtherServer => f.write_str("the server's key is not the one given"),
            TermsMismatch::OtherTickRate => {
                f.write_str("the server runs another tick rate than the one given")
            }
            TermsMismatch::Terms(error) => write!(f, "{error}"),
            TermsMismatch::OtherSession => {
                f.write_str("the server offers another session than the player's")
            }
        }
    }
}

impl Error for TermsMismatch {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            TermsMismatch::Terms(error) => Some(error),
            TermsMismatch::OtherServer
            | TermsMismatch::OtherTickRate
            | TermsMismatch::OtherSession => None,
        }
    }
}

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
            OrderScriptError::Action { tick } => {
                write!(
                    f,
                    "the order at tick {tick} names no action or more than one"
                )
            }
            OrderScriptError::EndsEarly { end } => {
                write!(f, "the script ends at tick {end}, before its last order")
            }
        }
    }
}

impl Error for OrderScriptError {}
