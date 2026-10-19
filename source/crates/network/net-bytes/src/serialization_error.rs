use std::io;

use campfire_codec::BinaryError;

/// Errors that can occur during serialization or deserialization.
#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum SerializationError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("Invalid value")]
    InvalidValue,
    /// A value that `codec` does not read, or whose bytes are not its one encoding.
    #[error(transparent)]
    Binary(#[from] BinaryError),
}
