use std::io;

use thiserror::Error;

use crate::platform::owner_only::Exposure;

/// Why a secret file did not read.
#[derive(Debug, Error)]
pub enum SecretReadError {
    /// No file holds its name.
    #[error("the file is not there")]
    Missing,
    /// It is a directory, a device or another file that holds no bytes of its own, as PostgreSQL
    /// refuses a key that is not a regular file.
    #[error("the file is not a regular file")]
    NotFile,
    /// Others than its owner may open it, as the OS names them.
    #[error("others may open the file ({0}); make it its owner's only")]
    Exposed(Exposure),
    /// It holds more bytes than its reader takes.
    #[error("the file holds more than {max} bytes")]
    TooLarge { max: usize },
    #[error("could not read the file")]
    Read(#[source] io::Error),
}
