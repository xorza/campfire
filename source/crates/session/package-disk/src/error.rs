use std::io;
use std::path::PathBuf;

use campfire_package_format::ContentError;
use campfire_store::{DurableDirCreateError, DurableDirError, DurableError, PathError, ReadError};
use thiserror::Error;

/// Why the files under a package's directory make no index, as its builder writes one.
#[derive(Debug, Error)]
pub enum IndexError {
    /// A directory or a file of the package does not read.
    #[error("a package's files do not read")]
    Read(#[source] PathError<ReadError>),
    /// An entry of a package is neither a file nor a directory, such as a link.
    #[error("{}: neither a file nor a directory", .0.display())]
    NotAFile(PathBuf),
    /// A path in a package is not UTF-8, so no file list can name it.
    #[error("{}: path is not UTF-8", .0.display())]
    NotUtf8(PathBuf),
    /// A file's name in a package is no package path, as one holding `\` is not.
    #[error("{}: no path a package can name", .0.display())]
    NotPath(PathBuf),
    /// The files make no package, as two paths that differ only in case.
    #[error("the files make no package")]
    Package(#[source] ContentError),
}

/// Why a package does not write.
#[derive(Debug, Error)]
pub enum WriteError {
    /// The package's directory does not make, as when one is there.
    #[error("the package's directory does not make")]
    CreateDir(#[source] PathError<DurableDirCreateError>),
    /// A directory in the package does not make.
    #[error("a directory of the package does not make")]
    Directory(#[source] PathError<DurableDirError>),
    /// A file of the package does not write, as one written twice.
    #[error("a file of the package does not write")]
    File(#[source] PathError<io::Error>),
    /// The package's index does not write.
    #[error("the package's index does not write")]
    Index(#[source] PathError<DurableError>),
    /// The files written make no package, as two paths that differ only in case.
    #[error("the files written make no package")]
    Package(#[source] ContentError),
}
