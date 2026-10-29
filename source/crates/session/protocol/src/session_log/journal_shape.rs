use std::ops::Range;

use crate::session_log::error::{PublishError, PublishWriteError, WalkStopped};
use crate::session_log::journal_kinds::{
    JOURNAL_CHECKPOINT_BEGUN, JOURNAL_CHECKPOINT_DONE, JOURNAL_HEADER, JOURNAL_LOADED,
    JOURNAL_PACKET, JOURNAL_RESULT, JOURNAL_SEALED, JOURNAL_SERVER,
};
use crate::session_log::take;

/// What gives a journal's records in order, each to the function it is given, at each call; an
/// error once it stopped short of the journal's end.
pub(crate) type JournalWalk<'w> = dyn FnMut(&mut dyn FnMut(&[u8])) -> Result<(), WalkStopped> + 'w;

/// What a journal holds of the log it publishes, each load's cut applied: the header, each
/// segment, the entries logged since the last tick was sealed, the result, and the records a load
/// dropped. A journal keeps it as it appends, and a restore as it reads the journal back, so a
/// publication reads the journal once (08-sessions.md, A published log is written from the
/// journal). Its counts are a tick's each, so it grows with the session's ticks, not its entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalShape {
    pub(crate) header: Option<Vec<u8>>,
    pub(crate) segments: Vec<SegmentShape>,
    pub(crate) tail: u32,
    pub(crate) result: Option<Vec<u8>>,
    /// The places among the records of the entries and seals each load dropped.
    cuts: Vec<Range<u64>>,
    /// The records it took.
    records: u64,
}

/// A segment as a journal holds it: the place of the record of its checkpoint's begin, none for
/// the first, its checkpoint's record and signature as the file holds them, and the count of the
/// entries of each of its sealed ticks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SegmentShape {
    begun: Option<u64>,
    pub(crate) record: Option<Vec<u8>>,
    pub(crate) ticks: Vec<u32>,
}

impl Default for JournalShape {
    fn default() -> Self {
        Self {
            header: None,
            segments: vec![SegmentShape {
                begun: None,
                record: None,
                ticks: Vec::new(),
            }],
            tail: 0,
            result: None,
            cuts: Vec::new(),
            records: 0,
        }
    }
}

impl JournalShape {
    /// The shape of the journal whose records `walk` gives, read in one walk, as of a journal no
    /// live log keeps; an error for a record that does not read.
    pub fn read(walk: &mut JournalWalk<'_>) -> Result<Self, PublishWriteError> {
        let mut shape = Self::default();
        let mut failed = None;
        walk(&mut |record| {
            if failed.is_none()
                && let Err(error) = shape.take(record)
            {
                failed = Some(error);
            }
        })?;
        match failed {
            Some(error) => Err(error.into()),
            None => Ok(shape),
        }
    }

    /// Takes the journal's next record, `record`; an error for a record of no kind, or that does
    /// not read.
    pub(crate) fn take(&mut self, record: &[u8]) -> Result<(), PublishError> {
        let at = self.records;
        self.records += 1;
        let (&kind, mut body) = record.split_first().ok_or(PublishError::Record { at })?;
        match kind {
            JOURNAL_HEADER => self.header = Some(body.to_vec()),
            JOURNAL_PACKET | JOURNAL_SERVER => self.tail += 1,
            JOURNAL_SEALED => {
                let last = self.segments.last_mut().expect("a log has a segment");
                last.ticks.push(self.tail);
                self.tail = 0;
            }
            JOURNAL_CHECKPOINT_BEGUN => self.segments.push(SegmentShape {
                begun: Some(at),
                record: None,
                ticks: Vec::new(),
            }),
            JOURNAL_CHECKPOINT_DONE => {
                let begun = self.segments.iter_mut().skip(1);
                let mut begun = begun.filter(|segment| segment.record.is_none());
                let segment = begun.next().ok_or(PublishError::Record { at })?;
                segment.record = Some(body.to_vec());
            }
            JOURNAL_LOADED => {
                let segment: u32 = take(&mut body).map_err(|_| PublishError::Record { at })?;
                let segment = usize::try_from(segment).expect("a u32 fits usize");
                let loaded = self.segments.get_mut(segment);
                let loaded = loaded.filter(|loaded| loaded.record.is_some());
                let loaded = loaded.ok_or(PublishError::Record { at })?;
                let begun = loaded.begun.expect("a segment loaded is past the first");
                loaded.ticks.clear();
                self.segments.truncate(segment + 1);
                self.tail = 0;
                self.cuts.push(begun + 1..at);
            }
            JOURNAL_RESULT => self.result = Some(body.to_vec()),
            _ => return Err(PublishError::Record { at }),
        }
        Ok(())
    }

    /// The journal's header; an error for a journal that holds none, or whose last checkpoint
    /// begun has no record yet, so its log is not whole.
    pub(crate) fn whole_header(&self) -> Result<&[u8], PublishError> {
        let header = self.header.as_deref().ok_or(PublishError::NoHeader)?;
        if self
            .segments
            .iter()
            .skip(1)
            .any(|segment| segment.record.is_none())
        {
            return Err(PublishError::Begun);
        }
        Ok(header)
    }

    /// How many records it took.
    pub(crate) const fn records(&self) -> u64 {
        self.records
    }

    /// Whether a load dropped the entry or the seal at place `at`.
    pub(crate) fn cut(&self, at: u64) -> bool {
        self.cuts.iter().any(|cut| cut.contains(&at))
    }
}
