use std::error::Error;
use std::fmt;
use std::io;

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
}

impl fmt::Display for ContentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContentError::OutsidePackage(path) => write!(f, "{path:?}: outside the package"),
            ContentError::Io { path, error } => write!(f, "{path}: {error}"),
            ContentError::Data { path, error } => write!(f, "{path}: {error}"),
        }
    }
}

impl Error for ContentError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ContentError::OutsidePackage(_) => None,
            ContentError::Io { error, .. } => Some(error),
            ContentError::Data { error, .. } => Some(error),
        }
    }
}
