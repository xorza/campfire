use campfire_protocol::NsecError;
use campfire_store::{DurableError, SecretReadError};
use thiserror::Error;

/// Why a key file did not read or write.
#[derive(Debug, Error)]
pub enum KeyFileError {
    #[error("the key file: {0}")]
    Read(#[source] SecretReadError),
    #[error("the key file: {0}")]
    NotNsec(#[source] NsecError),
    #[error("could not write the key file: {0}")]
    Write(#[source] DurableError),
}
