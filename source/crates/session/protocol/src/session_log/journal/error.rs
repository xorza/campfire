use std::io;

use thiserror::Error;

/// The bytes do not start with the journal's tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("not a journal")]
pub struct NotJournal;

/// Why a stream of a journal's frames does not read.
#[derive(Debug, Error)]
pub enum JournalStreamError {
    #[error(transparent)]
    NotJournal(#[from] NotJournal),
    /// The reader failed.
    #[error("the journal does not read")]
    Read(#[source] io::Error),
}
