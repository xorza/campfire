use campfire_common::Version;

use crate::session_log::applied::Applied;
use crate::session_log::error::{JournalReplayError, LogError};
use crate::session_log::journal_records::{JOURNAL_CHECKPOINT_BEGUN, JOURNAL_HEADER};
use crate::session_log::{SessionLog, file};

/// A log rebuilt from its journal one record at a time, in order, as the server took them: so a
/// restore streams the journal's file through a buffer of one record, and holds the log alone
/// (08-sessions.md, A published log is written from the journal).
#[derive(Debug)]
pub struct JournalReplay {
    log: SessionLog,
    applied: Vec<Applied>,
    /// The index of the next record, from 0, the header's.
    next: u64,
}

impl JournalReplay {
    /// Starts the log from the journal's first record, `first`; an error for a record that is not
    /// the header, or a header of a release other than `release`, the one that reads it, whose
    /// records are then not read, as their format is that release's.
    pub fn begin(first: &[u8], release: Version) -> Result<Self, JournalReplayError> {
        let failed = |error| JournalReplayError::Record { record: 0, error };
        let mut rest = first
            .strip_prefix(&[JOURNAL_HEADER])
            .ok_or(JournalReplayError::NoHeader)?;
        let header = file::take_header(&mut rest).map_err(failed)?;
        if !rest.is_empty() {
            return Err(failed(LogError::Trailing));
        }
        if header.terms.release != release {
            return Err(JournalReplayError::OtherRelease(header.terms.release));
        }
        let log = SessionLog::new(header).map_err(|error| failed(LogError::Header(error)))?;
        Ok(Self {
            log,
            applied: Vec::new(),
            next: 1,
        })
    }

    /// Takes the journal's next record, `record`; an error for a record after the result, a
    /// checkpoint's begin after an entry of its tick, and a record that does not decode or that
    /// the log refuses.
    pub fn apply(&mut self, record: &[u8]) -> Result<(), JournalReplayError> {
        let index = self.next;
        self.next += 1;
        if self.log.result.is_some() {
            return Err(JournalReplayError::AfterResult { record: index });
        }
        if record.first() == Some(&JOURNAL_CHECKPOINT_BEGUN) && !self.log.is_at_boundary() {
            return Err(JournalReplayError::CheckpointMidTick { record: index });
        }
        self.log
            .apply_journal(record, &mut self.applied)
            .map_err(|error| JournalReplayError::Record {
                record: index,
                error,
            })
    }

    /// The log the records taken rebuild: what the server had logged when it wrote the last.
    pub fn finish(self) -> SessionLog {
        self.log
    }
}

impl SessionLog {
    /// The log of the session whose journal holds `records`, in order, as the server took them,
    /// as `JournalReplay` rebuilds it record by record; an error as `JournalReplay` gives one,
    /// and for no record.
    pub fn from_journal<'a>(
        records: impl IntoIterator<Item = &'a [u8]>,
        release: Version,
    ) -> Result<Self, JournalReplayError> {
        let mut records = records.into_iter();
        let first = records.next().ok_or(JournalReplayError::NoHeader)?;
        let mut replay = JournalReplay::begin(first, release)?;
        for record in records {
            replay.apply(record)?;
        }
        Ok(replay.finish())
    }
}
