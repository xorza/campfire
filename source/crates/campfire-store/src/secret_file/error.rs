use std::io;

use thiserror::Error;

use crate::platform::owner_only::Exposure;

/// Why a secret file did not read.
#[derive(Debug, Error)]
pub enum SecretReadError {
    #[error("could not read the file")]
    Read(#[source] io::Error),
    /// Others than its owner may open it, as the OS names them.
    #[error("others may open the file ({0}); make it its owner's only")]
    Exposed(Exposure),
}
