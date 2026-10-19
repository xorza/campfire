use std::io;

/// Errors that can occur during serialization or deserialization.
#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum SerializationError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("Invalid value")]
    InvalidValue,
    /// A value in an encoding longer than the one it takes.
    #[error("a value in an encoding other than its one encoding")]
    NotCanonical,
}
