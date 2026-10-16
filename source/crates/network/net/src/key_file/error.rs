use campfire_protocol::NsecError;
use campfire_store::{DurableError, PathError, ReadError};
use thiserror::Error;

/// Why a key file did not read or write.
#[derive(Debug, Error)]
pub enum KeyFileError {
    #[error("the key file")]
    Read(#[source] PathError<ReadError>),
    #[error("the key file")]
    NotNsec(#[source] NsecError),
    #[error("could not write the key file")]
    Write(#[source] PathError<DurableError>),
}
