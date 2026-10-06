use std::error::Error;
use std::fmt;
use std::io;

use nostr::error::Error as NostrError;

use crate::durable_file::error::DurableError;

/// Why a key file did not read or write.
#[derive(Debug)]
pub enum KeyFileError {
    Read(io::Error),
    /// Others than its owner may read it: the bits of its mode, `0o644` as an example.
    Exposed {
        mode: u32,
    },
    NotNsec(NostrError),
    Write(DurableError),
}

impl fmt::Display for KeyFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeyFileError::Read(error) => write!(f, "could not read the key file: {error}"),
            KeyFileError::Exposed { mode } => {
                write!(
                    f,
                    "others may read the key file (mode {mode:o}); make it 600"
                )
            }
            KeyFileError::NotNsec(error) => write!(f, "the key file holds no nsec: {error}"),
            KeyFileError::Write(error) => write!(f, "could not write the key file: {error}"),
        }
    }
}

impl Error for KeyFileError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            KeyFileError::Read(error) => Some(error),
            KeyFileError::Exposed { .. } => None,
            KeyFileError::NotNsec(error) => Some(error),
            KeyFileError::Write(error) => Some(error),
        }
    }
}
