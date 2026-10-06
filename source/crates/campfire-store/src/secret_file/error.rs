use std::error::Error;
use std::fmt;
use std::io;

/// Why a secret file did not read.
#[derive(Debug)]
pub enum SecretReadError {
    Read(io::Error),
    /// Others than its owner may read it: the bits of its mode, `0o644` as an example.
    Exposed {
        mode: u32,
    },
}

impl fmt::Display for SecretReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SecretReadError::Read(error) => write!(f, "could not read the file: {error}"),
            SecretReadError::Exposed { mode } => {
                write!(f, "others may read the file (mode {mode:o}); make it 600")
            }
        }
    }
}

impl Error for SecretReadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            SecretReadError::Read(error) => Some(error),
            SecretReadError::Exposed { .. } => None,
        }
    }
}
