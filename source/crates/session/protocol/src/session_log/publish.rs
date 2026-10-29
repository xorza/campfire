//! The published log, written from the log's journal in one walk of its records.

use std::io::{self, Write};
use std::mem;

use campfire_codec::Binary;
use serde::Serialize;

use crate::server_seed::ServerSeed;
use crate::session_log::error::PublishWriteError;
use crate::session_log::file::{LOG_TAG, PACKET_ENTRY, SERVER_ENTRY};
use crate::session_log::journal_kinds::{JOURNAL_PACKET, JOURNAL_SEALED, JOURNAL_SERVER};
use crate::session_log::journal_shape::{JournalShape, JournalWalk};
use crate::session_log::{SessionLog, offset};

/// Where the walk stands in the file it writes: the segment and its tick it writes, and
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

/// The published file as it goes to its writer: each value encoded into one scratch buffer and
/// written on; the first write that fails stops the rest, and its error ends the publication.
struct PublishOut<'w> {
    writer: &'w mut dyn Write,
    scratch: Vec<u8>,
    failed: Option<io::Error>,
}

impl PublishOut<'_> {
    fn bytes(&mut self, bytes: &[u8]) {
        if self.failed.is_none()
            && let Err(error) = self.writer.write_all(bytes)
        {
            self.failed = Some(error);
        }
    }

    fn byte(&mut self, byte: u8) {
        self.bytes(&[byte]);
    }

    fn put<T: Serialize + ?Sized>(&mut self, value: &T) {
        let mut scratch = mem::take(&mut self.scratch);
        scratch.clear();
        Binary::encode_into(value, &mut scratch);
        self.bytes(&scratch);
        self.scratch = scratch;
    }
}

impl SessionLog {
    /// Writes into `writer` the file of the log whose journal `walk` gives and whose shape is
    /// `shape`, the journal's as of the records `walk` gives, with the server seed `revealed` of
    /// its last segment: the bytes `encode` writes for the log the journal rebuilds, each load's
    /// cut applied (08-sessions.md, A published log is written from the journal). `walk` gives the
    /// journal's records in order, once, so the memory a publication takes is a record's and the
    /// shape's. An error for a journal that holds no header, a checkpoint begun with no record, a
    /// walk that stopped short, so no log goes out with records missing, and a write that fails.
    pub fn publish(
        shape: &JournalShape,
        walk: &mut JournalWalk<'_>,
        revealed: &ServerSeed,
        writer: &mut dyn Write,
    ) -> Result<(), PublishWriteError> {
        let header = shape.whole_header()?;
        let out = &mut PublishOut {
            writer,
            scratch: Vec::new(),
            failed: None,
        };
        out.bytes(LOG_TAG);
        out.bytes(header);
        // A published log is the whole session's: its first segment is 0.
        out.put(&0_u32);
        out.put(&offset(shape.segments.len()));
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
                    cursor.enter(shape, out);
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
            cursor.enter(shape, out);
            let open = cursor.open.as_mut().expect("an entry goes in a tick");
            *open -= 1;
            out.byte(entry);
            out.bytes(body);
        })?;
        debug_assert_eq!(
            index,
            shape.records(),
            "a shape is of the records the walk gives"
        );
        cursor.finish(shape, out);
        out.put(&Some(revealed));
        match &shape.result {
            Some(result) => {
                out.byte(1);
                out.bytes(result);
            }
            None => out.byte(0),
        }
        out.failed
            .take()
            .map_or(Ok(()), |error| Err(PublishWriteError::Write(error)))
    }
}

impl Cursor {
    /// Opens the next place an entry goes, when none is open: the next tick of the segment, past
    /// those it wrote, its segment's own counts first, or, past the last segment's last tick, the
    /// entries since the last tick.
    fn enter(&mut self, shape: &JournalShape, out: &mut PublishOut<'_>) {
        if self.open.is_some() || self.tail {
            return;
        }
        loop {
            let Some(segment) = shape.segments.get(self.segment) else {
                self.tail = true;
                self.open = Some(shape.tail);
                out.put(&shape.tail);
                return;
            };
            if !self.entered {
                if let Some(record) = &segment.record {
                    out.bytes(record);
                }
                out.put(&u64::try_from(segment.ticks.len()).expect("ticks fit u64"));
                self.entered = true;
            }
            if let Some(&count) = segment.ticks.get(self.tick) {
                self.open = Some(count);
                out.put(&count);
                return;
            }
            self.segment += 1;
            self.tick = 0;
            self.entered = false;
        }
    }

    /// Writes the counts of what follows the last record: the segments of no tick after it, and
    /// the entries since the last tick.
    fn finish(&mut self, shape: &JournalShape, out: &mut PublishOut<'_>) {
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
