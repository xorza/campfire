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
    /// The mode input at `tick` takes a payload of `len` bytes, past the least a session's terms
    /// let a payload hold, so a session might refuse it (05-protocol-spec.md, A session's terms
    /// hold every input).
    #[error(
        "the mode input at tick {tick} takes {len} bytes, past the {least} every session holds"
    )]
    InputTooLarge { tick: u64, len: usize, least: u32 },
}
