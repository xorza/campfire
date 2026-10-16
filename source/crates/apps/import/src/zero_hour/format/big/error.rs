use campfire_store::{PathError, ReadError};
use thiserror::Error;

/// Why an archive's bytes are no `.big` archive.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ArchiveError {
    /// It does not start with `BIGF`.
    #[error("it does not start with BIGF")]
    NotBig,
    /// Its header, or its list of entries, ends past the file or its own length.
    #[error("its header ends past its length")]
    Short,
    /// An entry's bytes lie past the file's end.
    #[error("{0}'s bytes lie past its end")]
    EntryPastEnd(String),
    /// An entry's path is not ASCII.
    #[error("an entry's path is not ASCII")]
    NotAscii,
}

/// Why an archive's entries do not read: its file does not read, or its bytes are no `.big`
/// archive.
#[derive(Debug, Error)]
pub(crate) enum BigReadError {
    #[error(transparent)]
    Read(PathError<ReadError>),
    #[error(transparent)]
    Archive(ArchiveError),
}
