//! The published log, written from the log's journal in two walks of its records.

use std::ops::Range;

use crate::server_seed::ServerSeed;
use crate::session_log::error::PublishError;
use crate::session_log::file::{LOG_TAG, PACKET_ENTRY, SERVER_ENTRY};
use crate::session_log::journal_records::{
    JOURNAL_CHECKPOINT_BEGUN, JOURNAL_CHECKPOINT_DONE, JOURNAL_HEADER, JOURNAL_LOADED,
    JOURNAL_PACKET, JOURNAL_RESULT, JOURNAL_SEALED, JOURNAL_SERVER,
};
use crate::session_log::{SessionLog, offset, put, take};

/// What gives a journal's records in order, each to the function it is given, at each call.
pub(crate) type JournalWalk<'w> = dyn FnMut(&mut dyn FnMut(&[u8])) + 'w;

/// What a first walk of a journal finds of the log it holds, each load's cut applied: the header,
/// each segment, the entries logged since the last tick was sealed, the result, and the records a
/// load dropped. Its counts are a tick's each, so it grows with the session's ticks, not its
/// entries.
#[derive(Debug, Default)]
struct Shape {
    header: Vec<u8>,
    segments: Vec<SegmentShape>,
    tail: u32,
    result: Option<Vec<u8>>,
    /// The places among the records of the entries and seals each load dropped.
    cuts: Vec<Range<u64>>,
}

/// A segment as a journal holds it: the place of the record of its checkpoint's begin, none for
/// the first, its checkpoint's record and signature as the file holds them, and the count of the
/// entries of each of its sealed ticks.
#[derive(Debug)]
struct SegmentShape {
    begun: Option<u64>,
    record: Option<Vec<u8>>,
    ticks: Vec<u32>,
}

/// Where the second walk stands in the file it writes: the segment and its tick it writes, and
/// the entries the tick still takes, while a tick is open.
#[derive(Debug, Default)]
struct Cursor {
    segment: usize,
    tick: usize,
    open: Option<u32>,
    /// Whether the entries since the last tick are open, which nothing comes after.
    tail: bool,
    /// Whether the segment `segment`'s own counts are written.
    entered: bool,
}

impl SessionLog {
    /// Writes into `out`, which is cleared first, the file of the log whose journal `walk` gives,
    /// with the server seed `revealed` of its last segment: the bytes `encode` writes for the log
    /// the journal rebuilds, each load's cut applied (08-sessions.md, A published log is written
    /// from the journal). `walk` gives the journal's records in order to the function it is given,
    /// twice: once to count what the file's counts need, once to write each record the file
    /// keeps, so the memory a publication takes is a record's and the counts'. An error for a
    /// journal that holds no header, a record that does not read, and a checkpoint begun with no
    /// record.
    pub fn publish(
        walk: &mut JournalWalk<'_>,
        revealed: &ServerSeed,
        out: &mut Vec<u8>,
    ) -> Result<(), PublishError> {
        let shape = Shape::of(walk)?;
        out.clear();
        out.extend_from_slice(LOG_TAG);
        out.extend_from_slice(&shape.header);
        put(out, &offset(shape.segments.len()));
        let mut cursor = Cursor::default();
        let mut index = 0;
        walk(&mut |record| {
            let at = index;
            index += 1;
            let Some((&kind, body)) = record.split_first() else {
                return;
            };
            let entry = match kind {
                JOURNAL_PACKET => PACKET_ENTRY,
                JOURNAL_SERVER => SERVER_ENTRY,
                JOURNAL_SEALED if !shape.cut(at) => {
                    cursor.enter(&shape, out);
                    debug_assert_eq!(cursor.open, Some(0), "a sealed tick took its entries");
                    cursor.open = None;
                    cursor.tick += 1;
                    return;
                }
                _ => return,
            };
            if shape.cut(at) {
                return;
            }
            cursor.enter(&shape, out);
            let open = cursor.open.as_mut().expect("an entry goes in a tick");
            *open -= 1;
            out.push(entry);
            out.extend_from_slice(body);
        });
        cursor.finish(&shape, out);
        put(out, &Some(revealed));
        match &shape.result {
            Some(result) => {
                out.push(1);
                out.extend_from_slice(result);
            }
            None => out.push(0),
        }
        Ok(())
    }
}

impl Shape {
    /// The shape of the log whose journal `walk` gives.
    fn of(walk: &mut JournalWalk<'_>) -> Result<Self, PublishError> {
        let mut shape = Self {
            segments: vec![SegmentShape {
                begun: None,
                record: None,
                ticks: Vec::new(),
            }],
            ..Self::default()
        };
        let mut header = false;
        let mut failed = None;
        let mut index = 0;
        walk(&mut |record| {
            let at = index;
            index += 1;
            if failed.is_some() {
                return;
            }
            if let Err(error) = shape.take(at, record, &mut header) {
                failed = Some(error);
            }
        });
        if let Some(error) = failed {
            return Err(error);
        }
        if !header {
            return Err(PublishError::NoHeader);
        }
        if shape
            .segments
            .iter()
            .skip(1)
            .any(|segment| segment.record.is_none())
        {
            return Err(PublishError::Begun);
        }
        Ok(shape)
    }

    /// Takes the record `record` at place `at` of the journal.
    fn take(&mut self, at: u64, record: &[u8], header: &mut bool) -> Result<(), PublishError> {
        let (&kind, mut body) = record.split_first().ok_or(PublishError::Record { at })?;
        match kind {
            JOURNAL_HEADER => {
                self.header = body.to_vec();
                *header = true;
            }
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

    /// Whether a load dropped the entry or the seal at place `at`.
    fn cut(&self, at: u64) -> bool {
        self.cuts.iter().any(|cut| cut.contains(&at))
    }
}

impl Cursor {
    /// Opens the next place an entry goes, when none is open: the next tick of the segment, past
    /// those it wrote, its segment's own counts first, or, past the last segment's last tick, the
    /// entries since the last tick.
    fn enter(&mut self, shape: &Shape, out: &mut Vec<u8>) {
        if self.open.is_some() || self.tail {
            return;
        }
        loop {
            let Some(segment) = shape.segments.get(self.segment) else {
                self.tail = true;
                self.open = Some(shape.tail);
                put(out, &shape.tail);
                return;
            };
            if !self.entered {
                if let Some(record) = &segment.record {
                    out.extend_from_slice(record);
                }
                put(
                    out,
                    &u64::try_from(segment.ticks.len()).expect("ticks fit u64"),
                );
                self.entered = true;
            }
            if let Some(&count) = segment.ticks.get(self.tick) {
                self.open = Some(count);
                put(out, &count);
                return;
            }
            self.segment += 1;
            self.tick = 0;
            self.entered = false;
        }
    }

    /// Writes the counts of what follows the last record: the segments of no tick after it, and
    /// the entries since the last tick.
    fn finish(&mut self, shape: &Shape, out: &mut Vec<u8>) {
        debug_assert!(
            self.tail || self.open.is_none(),
            "only the entries since the last tick stay open"
        );
        self.enter(shape, out);
        debug_assert_eq!(
            self.open,
            Some(0),
            "the entries since the last tick come last"
        );
    }
}
