use thiserror::Error;

use crate::session_log::error::LogError;

/// The bytes do not start with the journal's tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("not a journal")]
pub struct NotJournal;

/// Why a journal's records do not rebuild a log. The server writes only records its log took, so
/// each is a fault of the journal's bytes, past the checks of its frames.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum JournalReplayError {
    /// The journal holds no record, or its first is not the header.
    #[error("the journal starts with no header")]
    NoHeader,
    /// The record of index `record`, from 0, comes after the result, which ends the session.
    #[error("journal record {record} comes after the session's result")]
    AfterResult { record: u64 },
    /// The record of index `record`, from 0, begins a checkpoint after an entry of the tick it
    /// comes before.
    #[error("journal record {record} begins a checkpoint after an entry of its tick")]
    CheckpointMidTick { record: u64 },
    /// The record of index `record`, from 0, does not decode, or the log refuses it.
    #[error("journal record {record}: {error}")]
    Record {
        record: u64,
        #[source]
        error: LogError,
    },
}
