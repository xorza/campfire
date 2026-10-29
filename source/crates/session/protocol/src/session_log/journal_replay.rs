use campfire_common::Version;

use crate::session_log::applied::Applied;
use crate::session_log::error::{JournalReplayError, LogError};
use crate::session_log::journal_kinds::{
    JOURNAL_CHECKPOINT_BEGUN, JOURNAL_CHECKPOINT_DONE, JOURNAL_HEADER, JOURNAL_LOADED,
};
use crate::session_log::journal_shape::JournalShape;
use crate::session_log::{SessionLog, file};

/// A log rebuilt from its journal one record at a time, in order, as the server took them, with
/// the journal's shape: so a restore streams the journal's file through a buffer of one record,
/// and holds the log and the shape alone (08-sessions.md, A published log is written from the
/// journal).
#[derive(Debug)]
pub struct JournalReplay {
    log: SessionLog,
    shape: JournalShape,
    applied: Vec<Applied>,
    /// The index of the next record, from 0, the header's.
    next: u64,
}

/// What a journal's records rebuild: the log the server had logged when it wrote the last, and
/// the journal's shape, which its journal keeps on from.
#[derive(Debug)]
pub struct ReplayedJournal {
    pub log: SessionLog,
    pub shape: JournalShape,
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
        let mut log = SessionLog::new(header).map_err(|error| failed(LogError::Header(error)))?;
        log.checks.defer();
        let mut shape = JournalShape::default();
        shape
            .take(first)
            .expect("a header the log reads is one a shape reads");
        Ok(Self {
            log,
            shape,
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
        self.log.checks.at_record(index);
        self.log
            .apply_journal(record, &mut self.applied)
            .map_err(|error| JournalReplayError::Record {
                record: index,
                error,
            })?;
        match record.first() {
            Some(&JOURNAL_CHECKPOINT_BEGUN) => self.log.checks.begin_checkpoint(),
            Some(&JOURNAL_CHECKPOINT_DONE) => self.log.checks.record_checkpoint(),
            Some(&JOURNAL_LOADED) => self.log.checks.load(),
            _ => {}
        }
        self.shape
            .take(record)
            .expect("a record the log takes is one a shape reads");
        Ok(())
    }

    /// What the records taken rebuild, once the signatures after the latest checkpoint whose
    /// record they hold check, those before it trusted as the server's own; an error for a
    /// signature that is not its signer's.
    pub fn finish(mut self) -> Result<ReplayedJournal, JournalReplayError> {
        self.log
            .checks
            .finish()
            .map_err(|record| JournalReplayError::BadSignature { record })?;
        Ok(ReplayedJournal {
            log: self.log,
            shape: self.shape,
        })
    }
}

impl ReplayedJournal {
    /// What the journal of `records`, in order, as the server took them, rebuilds, as
    /// `JournalReplay` rebuilds it record by record; an error as `JournalReplay` gives one, and
    /// for no record.
    pub fn of<'a>(
        records: impl IntoIterator<Item = &'a [u8]>,
        release: Version,
    ) -> Result<Self, JournalReplayError> {
        let mut records = records.into_iter();
        let first = records.next().ok_or(JournalReplayError::NoHeader)?;
        let mut replay = JournalReplay::begin(first, release)?;
        for record in records {
            replay.apply(record)?;
        }
        replay.finish()
    }
}

impl SessionLog {
    /// The log of the session whose journal holds `records`, as `ReplayedJournal::of` rebuilds
    /// it.
    pub fn from_journal<'a>(
        records: impl IntoIterator<Item = &'a [u8]>,
        release: Version,
    ) -> Result<Self, JournalReplayError> {
        ReplayedJournal::of(records, release).map(|replayed| replayed.log)
    }
}
