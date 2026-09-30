use std::error::Error;
use std::fmt;
use std::io;
use std::path::PathBuf;

use toml::de::Error as TomlError;

use crate::package_path::PackagePath;

/// Why a package file does not load. Packages are untrusted, so each is an expected failure.
#[derive(Debug)]
pub enum ContentError {
    /// The path, as given, leaves the package: empty, absolute, or through `..`.
    OutsidePackage(String),
    /// The file does not read.
    Io { path: PackagePath, error: io::Error },
    /// The data file is not TOML of the expected shape.
    Data { path: PackagePath, error: TomlError },
    /// A directory of packages, or of a package's files, does not read.
    Scan { dir: PathBuf, error: io::Error },
    /// An entry of a package is neither a file nor a directory, such as a link.
    NotAFile(PathBuf),
    /// A path in a package is not UTF-8, so no file list can name it.
    NotUtf8(PathBuf),
}

impl fmt::Display for ContentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContentError::OutsidePackage(path) => write!(f, "{path:?}: outside the package"),
            ContentError::Io { path, error } => write!(f, "{path}: {error}"),
            ContentError::Data { path, error } => write!(f, "{path}: {error}"),
            ContentError::Scan { dir, error } => write!(f, "{}: {error}", dir.display()),
            ContentError::NotAFile(path) => {
                write!(f, "{}: neither a file nor a directory", path.display())
            }
            ContentError::NotUtf8(path) => write!(f, "{}: path is not UTF-8", path.display()),
        }
    }
}

impl Error for ContentError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ContentError::OutsidePackage(_)
            | ContentError::NotAFile(_)
            | ContentError::NotUtf8(_) => None,
            ContentError::Io { error, .. } | ContentError::Scan { error, .. } => Some(error),
            ContentError::Data { error, .. } => Some(error),
        }
    }
}
