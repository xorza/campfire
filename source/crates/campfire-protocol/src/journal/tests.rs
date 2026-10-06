use std::sync::{Arc, Mutex};

use super::*;
use crate::journal::error::NotJournal;
use crate::journal::journal_frames::MAX_RECORD;

/// A journal's file in memory, which a test reads back as it likes.
#[derive(Debug, Clone)]
pub(crate) struct MemorySink(pub(crate) Arc<Mutex<Vec<u8>>>);

impl MemorySink {
    /// A file that holds the journal's tag alone, as a new journal does.
    pub(crate) fn new() -> MemorySink {
        MemorySink(Arc::new(Mutex::new(JournalFrames::TAG.to_vec())))
    }

    pub(crate) fn bytes(&self) -> Vec<u8> {
        self.0.lock().unwrap().clone()
    }

    /// A box of a sink into the same bytes, for a log to keep.
    pub(crate) fn boxed(&self) -> Box<dyn RecordSink + Send + Sync> {
        Box::new(self.clone())
    }
}

impl RecordSink for MemorySink {
    fn append(&self, write: &mut dyn FnMut(&mut Vec<u8>)) {
        write(&mut self.0.lock().unwrap());
    }
}

/// Three records, the second empty.
const RECORDS: [&[u8]; 3] = [b"first", b"", b"third record"];

/// The journal file of `RECORDS`, written in turn.
fn journal_of_records() -> Vec<u8> {
    let file = MemorySink::new();
    let mut journal = Journal::new(file.boxed());
    for record in RECORDS {
        journal.append(|out| out.extend_from_slice(record));
    }
    assert_eq!(journal.written(), 3);
    file.bytes()
}

/// The records a read of `bytes` gives, and where it stops.
fn read(bytes: &[u8]) -> Option<(Vec<&[u8]>, usize)> {
    let mut frames = JournalFrames::new(bytes).ok()?;
    let records = frames.by_ref().collect();
    Some((records, frames.whole()))
}

#[test]
fn frames_round_trip_and_a_cut_reads_to_its_last_whole_frame() {
    let bytes = journal_of_records();
    // Each frame: a u32 length, the record, and its 32-byte hash.
    let ends = [5, 0, 12].iter().scan(JournalFrames::TAG.len(), |at, len| {
        *at += 4 + len + 32;
        Some(*at)
    });
    let ends: Vec<usize> = ends.collect();
    assert_eq!(ends.last(), Some(&bytes.len()));
    assert_eq!(read(&bytes), Some((RECORDS.to_vec(), bytes.len())));
    assert_eq!(
        &bytes[JournalFrames::TAG.len()..JournalFrames::TAG.len() + 4],
        &5_u32.to_le_bytes()
    );
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"campfire/journal-frame/v1").update(b"first");
    let first_hash = JournalFrames::TAG.len() + 4 + 5;
    assert_eq!(
        &bytes[first_hash..first_hash + 32],
        hasher.finalize().as_bytes()
    );

    // Another version's tag is no journal.
    assert_eq!(
        JournalFrames::new(b"campfire/journal/v0").err(),
        Some(NotJournal)
    );

    // Cut at every byte: short of the tag it is no journal; past it, the read gives the frames
    // that end by the cut, and stops where the last of them ends.
    for at in 0..bytes.len() {
        let cut = read(&bytes[..at]);
        if at < JournalFrames::TAG.len() {
            assert_eq!(cut, None, "cut at {at}");
            continue;
        }
        let whole = ends.iter().take_while(|&&end| end <= at).count();
        let stop = if whole == 0 {
            JournalFrames::TAG.len()
        } else {
            ends[whole - 1]
        };
        assert_eq!(cut, Some((RECORDS[..whole].to_vec(), stop)), "cut at {at}");
    }
}

#[test]
fn a_flipped_byte_or_a_length_past_the_bound_stops_the_read_at_its_frame() {
    let bytes = journal_of_records();
    let starts = [
        JournalFrames::TAG.len(),
        JournalFrames::TAG.len() + 4 + 5 + 32,
        JournalFrames::TAG.len() + 4 + 5 + 32 + 4 + 32,
    ];
    for at in JournalFrames::TAG.len()..bytes.len() {
        let mut flipped = bytes.clone();
        flipped[at] ^= 0x01;
        let frame = starts.iter().rposition(|&start| start <= at).unwrap();
        let read = read(&flipped).unwrap();
        assert_eq!(
            read,
            (RECORDS[..frame].to_vec(), starts[frame]),
            "byte {at}"
        );
    }
    // A length one past the bound, with that many bytes after it, is a bad frame.
    let mut long = JournalFrames::TAG.to_vec();
    let start = JournalFrames::open(&mut long);
    long.resize(long.len() + MAX_RECORD + 1, 0);
    let len = u32::try_from(MAX_RECORD + 1).unwrap();
    long[start..start + 4].copy_from_slice(&len.to_le_bytes());
    long.extend_from_slice(&JournalFrames::hash(&long[start + 4..]));
    assert_eq!(read(&long), Some((Vec::new(), JournalFrames::TAG.len())));
}
