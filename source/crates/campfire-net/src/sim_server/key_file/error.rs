use std::error::Error;
use std::fmt;

use campfire_protocol::NsecError;
use campfire_store::{DurableError, SecretReadError};

/// Why a key file did not read or write.
#[derive(Debug)]
pub enum KeyFileError {
    Read(SecretReadError),
    NotNsec(NsecError),
    Write(DurableError),
}

impl fmt::Display for KeyFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeyFileError::Read(error) => write!(f, "the key file: {error}"),
            KeyFileError::NotNsec(error) => write!(f, "the key file: {error}"),
            KeyFileError::Write(error) => write!(f, "could not write the key file: {error}"),
        }
    }
}

impl Error for KeyFileError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            KeyFileError::Read(error) => Some(error),
            KeyFileError::NotNsec(error) => Some(error),
            KeyFileError::Write(error) => Some(error),
        }
    }
}
