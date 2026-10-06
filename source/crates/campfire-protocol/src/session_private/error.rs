use std::error::Error;
use std::fmt;

/// Why a file's bytes are no private record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionPrivateError {
    /// The bytes do not start with the record's tag.
    NotPrivate,
    Malformed(postcard::Error),
    /// Bytes remain after the record.
    Trailing,
}

impl fmt::Display for SessionPrivateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SessionPrivateError::NotPrivate => f.write_str("not a session's private record"),
            SessionPrivateError::Malformed(error) => write!(f, "does not decode: {error}"),
            SessionPrivateError::Trailing => f.write_str("bytes after the record"),
        }
    }
}

impl Error for SessionPrivateError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            SessionPrivateError::Malformed(error) => Some(error),
            SessionPrivateError::NotPrivate | SessionPrivateError::Trailing => None,
        }
    }
}
