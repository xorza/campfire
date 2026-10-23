use campfire_codec::TomlError;
use thiserror::Error;

/// Why an order script does not read.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OrderScriptError {
    #[error(transparent)]
    Toml(TomlError),
    /// The order at `tick` comes after an order at a later tick.
    #[error("the order at tick {tick} comes after a later one")]
    Unordered { tick: u64 },
    /// The script ends at `end`, before its last order.
    #[error("the script ends at tick {end}, before its last order")]
    EndsEarly { end: u64 },
}
