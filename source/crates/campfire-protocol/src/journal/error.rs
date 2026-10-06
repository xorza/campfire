use std::error::Error;
use std::fmt;
use std::io;

use crate::durable_file::error::DurableError;
use crate::session_log::error::LogError;

/// Why a journal did not open.
#[derive(Debug)]
pub enum JournalError {
    /// The new journal's tag was not written durably.
    Create(DurableError),
    Open(io::Error),
    /// The bytes do not start with the journal's tag.
    NotJournal,
}

impl fmt::Display for JournalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JournalError::Create(error) => write!(f, "could not make the journal: {error}"),
            JournalError::Open(error) => write!(f, "could not open the journal: {error}"),
            JournalError::NotJournal => f.write_str("not a journal"),
        }
    }
}

impl Error for JournalError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            JournalError::Create(error) => Some(error),
            JournalError::Open(error) => Some(error),
            JournalError::NotJournal => None,
        }
    }
}

/// Why a journal's records do not rebuild a log. The server writes only records its log took, so
/// each is a fault of the journal's bytes, past the checks of its frames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JournalReplayError {
    /// The journal holds no record, or its first is not the header.
    NoHeader,
    /// The record of index `record`, from 0, does not decode, or the log refuses it.
    Record { record: u64, error: LogError },
}

impl fmt::Display for JournalReplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JournalReplayError::NoHeader => f.write_str("the journal starts with no header"),
            JournalReplayError::Record { record, error } => {
                write!(f, "journal record {record}: {error}")
            }
        }
    }
}

impl Error for JournalReplayError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            JournalReplayError::NoHeader => None,
            JournalReplayError::Record { error, .. } => Some(error),
        }
    }
}
