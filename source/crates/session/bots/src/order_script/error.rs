use std::path::PathBuf;

use campfire_codec::TomlError;
use campfire_common::DeclaredName;
use campfire_store::{PathError, ReadError};
use thiserror::Error;

/// Why an order script does not read.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OrderScriptError {
    #[error(transparent)]
    Toml(TomlError),
    /// The order at `tick` comes after an order at a later tick.
    #[error("the order at tick {tick} comes after a later one")]
    Unordered { tick: u64 },
    /// The order at `tick` names no action.
    #[error("the order at tick {tick} names no action")]
    NoAction { tick: u64 },
    /// The order at `tick` names more than one action.
    #[error("the order at tick {tick} names more than one action")]
    ManyActions { tick: u64 },
    /// The order at `tick` names a target, which only a cast takes, and no cast.
    #[error("the order at tick {tick} names a target and no cast")]
    StrayTarget { tick: u64 },
    /// The script ends at `end`, before its last order.
    #[error("the script ends at tick {end}, before its last order")]
    EndsEarly { end: u64 },
}

/// Why an order script's file does not give a script.
#[derive(Debug, Error)]
pub enum OrderScriptReadError {
    #[error("the order script does not read")]
    Read(#[source] PathError<ReadError>),
    #[error("{} holds no order script", .path.display())]
    Script {
        path: PathBuf,
        #[source]
        error: OrderScriptError,
    },
}

/// A bot's script names a unit type the mode does not have.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("the orders name the unit type {name}, which the mode does not have")]
pub struct UnknownUnitType {
    pub name: DeclaredName,
}
