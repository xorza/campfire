use std::io;
use std::str::Utf8Error;

use thiserror::Error;

use crate::platform::owner_only::Exposure;

/// Why a file or a directory did not read. One error serves every read, so a caller matches one
/// type: `NotText` comes from `InputFile::read_text` alone, `NotDir` from `DirEntries::read`
/// alone, and `Exposed` from `SecretFile::read` alone.
#[derive(Debug, Error)]
pub enum ReadError {
    /// No file or directory holds its name.
    #[error("it is not there")]
    Missing,
    /// It is a directory, a device or another file that holds no bytes of its own, as PostgreSQL
    /// refuses a key that is not a regular file.
    #[error("it is not a regular file")]
    NotFile,
    /// A listing's path is not a directory.
    #[error("it is not a directory")]
    NotDir,
    /// It holds more bytes than its reader takes.
    #[error("it holds more than {max} bytes")]
    TooLarge { max: usize },
    /// Its bytes are not UTF-8.
    #[error("it is not UTF-8 text")]
    NotText(#[source] Utf8Error),
    /// Others than its owner may open a secret file, as the OS names them.
    #[error("others may open it ({0}); make it its owner's only")]
    Exposed(Exposure),
    #[error("it did not read")]
    Read(#[source] io::Error),
}
