use std::io;
use std::path::PathBuf;

use thiserror::Error;
use toml::de::Error as TomlError;

/// Why an order script does not read.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OrderScriptError {
    #[error(transparent)]
    Toml(TomlError),
    /// A coordinate of the order at `tick` is past what a sim number holds.
    #[error("the order at tick {tick} has a coordinate past a sim number")]
    Coordinate { tick: u64 },
    /// The order at `tick` comes after an order at a later tick.
    #[error("the order at tick {tick} comes after a later one")]
    Unordered { tick: u64 },
    /// The order at `tick` names no action, or more than one.
    #[error("the order at tick {tick} names no action or more than one")]
    Action { tick: u64 },
    /// The script ends at `end`, before its last order.
    #[error("the script ends at tick {end}, before its last order")]
    EndsEarly { end: u64 },
}

/// Why an order script's file does not give a script.
#[derive(Debug, Error)]
pub enum OrderScriptReadError {
    #[error("{} does not read", .path.display())]
    Read {
        path: PathBuf,
        #[source]
        error: io::Error,
    },
    #[error("{} holds no order script", .path.display())]
    Script {
        path: PathBuf,
        #[source]
        error: OrderScriptError,
    },
}
