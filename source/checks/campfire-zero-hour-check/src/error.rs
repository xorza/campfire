use campfire_capabilities::PackagePath;
use campfire_common::BinaryError;
use campfire_import::ImportError;
use campfire_package::{ClassTextureError, ContentError, LoadError};
use campfire_store::{DurableCreateError, DurableError, PathError};
use thiserror::Error;

/// Why the check did not run to its verdict.
#[derive(Debug, Error)]
pub(crate) enum CheckError {
    /// `GENERALSZH_DATA` names no install, so no run checks one.
    #[error("GENERALSZH_DATA names no Zero Hour install")]
    NoInstall,
    /// The run's directory, or its root, does not make.
    #[error("the run's directory does not make")]
    RunRoot(#[source] PathError<DurableError>),
    #[error("the run's directory does not make")]
    RunDir(#[source] PathError<DurableCreateError>),
    #[error("the install does not import")]
    Import(#[source] ImportError),
    #[error("the imported mode does not load")]
    Load(#[source] LoadError),
    /// A file of the imported package does not read.
    #[error("a file of the imported package does not read")]
    Read(#[source] ContentError),
    #[error("a grid of the imported package does not decode")]
    Decode(#[source] BinaryError),
    /// A texture class's texture is none the atlas takes.
    #[error("{path} is no terrain texture")]
    Texture {
        path: PackagePath,
        #[source]
        error: ClassTextureError,
    },
}
