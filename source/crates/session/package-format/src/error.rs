use std::error::Error;
use std::str::Utf8Error;

use campfire_codec::{BinaryError, TomlError};
use campfire_values::PackagePath;
use thiserror::Error;

/// Why a package file does not load. Packages are untrusted, so each is an expected failure.
#[derive(Debug, Error)]
pub enum ContentError {
    /// The package holds no file at the path.
    #[error("{path} is missing")]
    Missing { path: PackagePath },
    /// The file is not UTF-8 text.
    #[error("{path} is not text")]
    NotText {
        path: PackagePath,
        #[source]
        error: Utf8Error,
    },
    /// The data file is not TOML of the expected shape.
    #[error("{path} does not parse as its data")]
    Data {
        path: PackagePath,
        #[source]
        error: TomlError,
    },
    /// The binary file does not decode as its type.
    #[error("{path} does not decode as its data")]
    Binary {
        path: PackagePath,
        #[source]
        error: BinaryError,
    },
    /// The source of the package's files does not read them, as a directory on disk fails; the
    /// source's own error, which the core does not name.
    #[error("a package's files do not read")]
    Read(#[source] Box<dyn Error + Send + Sync>),
    /// Two paths of a package, or the directories on their way, differ only in case, which a
    /// file system that ignores case holds as one, as Windows' and macOS's do by default.
    #[error("{path} and {other} differ only in case")]
    CaseClash {
        path: PackagePath,
        other: PackagePath,
    },
    /// A file's size or SHA-256 differs from its row of the package's index.
    #[error("{path} differs from the package's index")]
    Changed { path: PackagePath },
    /// The package's index does not start with the line of this version of its format's tag.
    #[error("the package's index is no index of this version")]
    IndexTag,
    /// A line of the package's index is no row of this version's one encoding, by its number,
    /// the tag's line 1.
    #[error("line {line} of the package's index is no row of this version")]
    IndexLine { line: usize },
    /// A row of the package's index names no package path.
    #[error("the package's index lists {0:?}, no path a package can name")]
    IndexPath(String),
    /// A row of the package's index is not past the one before it in the order of paths' bytes.
    #[error("the package's index lists {0} out of order")]
    IndexOrder(PackagePath),
    /// The package's index lists a file of more bytes than a package's file holds, whose read
    /// would allocate them.
    #[error("the package's index lists {0} of more than a package file's bytes")]
    IndexFileLen(PackagePath),
    /// A walk of the package's files took the path twice.
    #[error("{0} is taken twice")]
    Repeated(PackagePath),
    /// The package's index lists itself, which no list can hold the hash of, or a path under its
    /// name, in any case.
    #[error("the package's index lists itself")]
    IndexListsItself,
    /// A path of the package names `name` a directory, where another names it a file.
    #[error("{path} names {name} a directory, and another path names it a file")]
    FileAndDir {
        path: PackagePath,
        name: PackagePath,
    },
    /// The package's index is more bytes than its read takes, `FileIndex::MAX_LEN`.
    #[error("the package's index is more bytes than its read takes")]
    IndexLen,
}
