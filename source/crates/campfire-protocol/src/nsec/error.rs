use std::error::Error;
use std::fmt;
use std::str::Utf8Error;

use nostr::error::Error as NostrError;

/// Why bytes hold no `nsec`.
#[derive(Debug)]
pub enum NsecError {
    NotText(Utf8Error),
    NotNsec(NostrError),
}

impl fmt::Display for NsecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NsecError::NotText(error) => write!(f, "the key is no text: {error}"),
            NsecError::NotNsec(error) => write!(f, "the key is no nsec: {error}"),
        }
    }
}

impl Error for NsecError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            NsecError::NotText(error) => Some(error),
            NsecError::NotNsec(error) => Some(error),
        }
    }
}
