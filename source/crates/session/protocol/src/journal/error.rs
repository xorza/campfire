use thiserror::Error;

/// The bytes do not start with the journal's tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("not a journal")]
pub struct NotJournal;
