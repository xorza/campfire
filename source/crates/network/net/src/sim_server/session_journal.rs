use std::path::Path;

use campfire_protocol::{JournalFrames, RecordSink};
use campfire_store::{AppendOpenError, AppendWatch, AppendWriter, PathError};
use campfire_threads::Job;

/// A session's private write-ahead journal: the store's append writer, into which the session log
/// frames each record it takes, on the worker `journal`.
#[derive(Debug)]
pub struct SessionJournal(pub(crate) AppendWriter);

impl SessionJournal {
    /// A new journal at `path`, holding its tag alone, written durably as a whole file.
    pub fn create(path: &Path) -> Result<Self, PathError<AppendOpenError>> {
        AppendWriter::create(Job::Journal, path, JournalFrames::TAG).map(SessionJournal)
    }

    /// The journal at `path`, cut to its first `whole` bytes, the tag and the frames a read found
    /// whole.
    pub fn reopen(path: &Path, whole: u64) -> Result<Self, PathError<AppendOpenError>> {
        AppendWriter::reopen(Job::Journal, path, whole).map(SessionJournal)
    }

    pub fn watch(&self) -> AppendWatch {
        self.0.watch()
    }
}

impl RecordSink for SessionJournal {
    fn append(&self, records: u64, write: &mut dyn FnMut(&mut Vec<u8>)) {
        self.0.append(records, write);
    }
}
