use std::error::Error;
use std::fmt;

use crate::session_log::error::LogError;

/// The bytes do not start with the journal's tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotJournal;

impl fmt::Display for NotJournal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("not a journal")
    }
}

impl Error for NotJournal {}

/// Why a journal's records do not rebuild a log. The server writes only records its log took, so
/// each is a fault of the journal's bytes, past the checks of its frames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JournalReplayError {
    /// The journal holds no record, or its first is not the header.
    NoHeader,
    /// The record of index `record`, from 0, comes after the result, which ends the session.
    AfterResult { record: u64 },
    /// The record of index `record`, from 0, begins a checkpoint after an entry of the tick it
    /// comes before.
    CheckpointMidTick { record: u64 },
    /// The record of index `record`, from 0, does not decode, or the log refuses it.
    Record { record: u64, error: LogError },
}

impl fmt::Display for JournalReplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JournalReplayError::NoHeader => f.write_str("the journal starts with no header"),
            JournalReplayError::AfterResult { record } => {
                write!(
                    f,
                    "journal record {record} comes after the session's result"
                )
            }
            JournalReplayError::CheckpointMidTick { record } => write!(
                f,
                "journal record {record} begins a checkpoint after an entry of its tick"
            ),
            JournalReplayError::Record { record, error } => {
                write!(f, "journal record {record}: {error}")
            }
        }
    }
}

impl Error for JournalReplayError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            JournalReplayError::NoHeader
            | JournalReplayError::AfterResult { .. }
            | JournalReplayError::CheckpointMidTick { .. } => None,
            JournalReplayError::Record { error, .. } => Some(error),
        }
    }
}
