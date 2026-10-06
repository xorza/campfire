use thiserror::Error;

/// Why a file's bytes are no private record.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SessionPrivateError {
    /// The bytes do not start with the record's tag.
    #[error("not a session's private record")]
    NotPrivate,
    #[error("does not decode")]
    Malformed(#[source] postcard::Error),
    /// Bytes remain after the record.
    #[error("bytes after the record")]
    Trailing,
}
