use campfire_codec::BinaryError;
use campfire_import::ZeroHourError;
use campfire_package::{ContentError, LoadError};
use campfire_store::RunDirError;
use thiserror::Error;

/// Why the check did not run to its verdict.
#[derive(Debug, Error)]
pub(crate) enum CheckError {
    /// `GENERALSZH_DATA` names no install, so no run checks one.
    #[error("GENERALSZH_DATA names no Zero Hour install")]
    NoInstall,
    #[error(transparent)]
    RunDir(#[from] RunDirError),
    #[error("the install does not import")]
    Import(#[source] ZeroHourError),
    #[error("the imported mode does not load")]
    Load(#[source] LoadError),
    /// A file of the imported package does not read.
    #[error("a file of the imported package does not read")]
    Read(#[source] ContentError),
    #[error("a grid of the imported package does not decode")]
    Decode(#[source] BinaryError),
}
