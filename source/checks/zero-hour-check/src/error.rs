use std::path::PathBuf;

use campfire_codec::{BinaryError, TomlError};
use campfire_import::ZeroHourError;
use campfire_package::LoadError;
use campfire_package_format::ContentError;
use campfire_store::{DurableDirError, DurableError, PathError, ReadError};
use thiserror::Error;

/// Why the check did not run to its verdict.
#[derive(Debug, Error)]
pub(crate) enum CheckError {
    /// `GENERALSZH_DATA` names no install, so no run checks one.
    #[error("GENERALSZH_DATA names no Zero Hour install")]
    NoInstall,
    /// The directory the packages go to has no name of its own, as `/` or `..`.
    #[error("{} names no directory the packages may go to", .0.display())]
    NoName(PathBuf),
    #[error("the directory the packages go to does not list")]
    List(#[source] PathError<ReadError>),
    /// The directory the packages go to holds what no run wrote, which the check keeps.
    #[error("{} holds more than the packages of an import", .0.display())]
    NotPackages(PathBuf),
    #[error("the packages of an earlier run are not removed")]
    Remove(#[source] PathError<DurableDirError>),
    #[error("the new packages do not take the place of the earlier ones")]
    Rename(#[source] PathError<DurableDirError>),
    #[error("the install does not import")]
    Import(#[source] ZeroHourError),
    #[error("the imported mode does not load")]
    Load(#[source] LoadError),
    /// A file of the imported package does not read.
    #[error("a file of the imported package does not read")]
    Read(#[from] ContentError),
    #[error("a grid of the imported package does not decode")]
    Decode(#[source] BinaryError),
    #[error("the golden file does not read")]
    GoldenRead(#[source] PathError<ReadError>),
    /// The golden file's bytes are no UTF-8 text.
    #[error("the golden file is no text")]
    GoldenText,
    #[error("the golden file is none the check reads")]
    GoldenParse(#[source] TomlError),
    #[error("the golden file does not write")]
    GoldenWrite(#[source] PathError<DurableError>),
}
