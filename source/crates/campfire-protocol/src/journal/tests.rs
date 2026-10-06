use std::fs;
use std::io::ErrorKind;

use super::*;
use crate::durable_file::tests::ScratchDir;
use crate::journal::journal_frames::MAX_RECORD;

/// A journal's file in memory, which a test reads back as it likes.
#[derive(Debug, Clone, Default)]
pub(crate) struct MemoryFile(pub(crate) Arc<Mutex<Vec<u8>>>);

impl MemoryFile {
    /// A file that holds the journal's tag alone, as a new journal does.
    pub(crate) fn new() -> MemoryFile {
        MemoryFile(Arc::new(Mutex::new(JOURNAL_TAG.to_vec())))
    }

    pub(crate) fn bytes(&self) -> Vec<u8> {
        self.0.lock().unwrap().clone()
    }
}

impl JournalFile for MemoryFile {
    fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(())
    }

    fn sync(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A file whose every sync fails.
#[derive(Debug)]
struct FailingFile;

impl JournalFile for FailingFile {
    fn append(&mut self, _: &[u8]) -> io::Result<()> {
        Ok(())
    }

    fn sync(&mut self) -> io::Result<()> {
        Err(io::Error::other("sync failed"))
    }
}

/// Three records, the second empty.
const RECORDS: [&[u8]; 3] = [b"first", b"", b"third record"];

/// The journal file of `RECORDS`, written in turn and synced.
fn journal_of_records() -> Vec<u8> {
    let file = MemoryFile::new();
    let journal = Journal::start(file.clone());
    let watch = journal.watch();
    for record in RECORDS {
        journal.append(|out| out.extend_from_slice(record));
    }
    drop(journal);
    assert_eq!(watch.durable(), 3);
    assert!(watch.take_failure().is_none());
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
    let ends = [5, 0, 12].iter().scan(JOURNAL_TAG.len(), |at, len| {
        *at += 4 + len + 32;
        Some(*at)
    });
    let ends: Vec<usize> = ends.collect();
    assert_eq!(ends.last(), Some(&bytes.len()));
    assert_eq!(read(&bytes), Some((RECORDS.to_vec(), bytes.len())));
    assert_eq!(
        &bytes[JOURNAL_TAG.len()..JOURNAL_TAG.len() + 4],
        &5_u32.to_le_bytes()
    );
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"campfire/journal-frame/v1").update(b"first");
    let first_hash = JOURNAL_TAG.len() + 4 + 5;
    assert_eq!(
        &bytes[first_hash..first_hash + 32],
        hasher.finalize().as_bytes()
    );

    // Cut at every byte: short of the tag it is no journal; past it, the read gives the frames
    // that end by the cut, and stops where the last of them ends.
    for at in 0..bytes.len() {
        let cut = read(&bytes[..at]);
        if at < JOURNAL_TAG.len() {
            assert_eq!(cut, None, "cut at {at}");
            continue;
        }
        let whole = ends.iter().take_while(|&&end| end <= at).count();
        let stop = if whole == 0 {
            JOURNAL_TAG.len()
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
        JOURNAL_TAG.len(),
        JOURNAL_TAG.len() + 4 + 5 + 32,
        JOURNAL_TAG.len() + 4 + 5 + 32 + 4 + 32,
    ];
    for at in JOURNAL_TAG.len()..bytes.len() {
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
    let mut long = JOURNAL_TAG.to_vec();
    let start = JournalFrames::open(&mut long);
    long.resize(long.len() + MAX_RECORD + 1, 0);
    let len = u32::try_from(MAX_RECORD + 1).unwrap();
    long[start..start + 4].copy_from_slice(&len.to_le_bytes());
    long.extend_from_slice(&JournalFrames::hash(&long[start + 4..]));
    assert_eq!(read(&long), Some((Vec::new(), JOURNAL_TAG.len())));
}

#[test]
fn a_writer_whose_file_fails_reports_the_failure_once_and_keeps_nothing_after() {
    let journal = Journal::start(FailingFile);
    let watch = journal.watch();
    journal.append(|out| out.extend_from_slice(b"lost"));
    // Dropped, the journal waits for its writer, which stopped at the failed sync.
    drop(journal);
    let failure = watch.take_failure().map(|error| error.kind());
    assert_eq!(failure, Some(ErrorKind::Other));
    assert!(watch.take_failure().is_none());
    assert_eq!(watch.durable(), 0);
}

#[test]
fn a_new_journal_on_disk_holds_its_tag_and_then_its_records() {
    let dir = ScratchDir::new("journal");
    let path = dir.0.join("journal");
    let journal = Journal::create(&path).unwrap();
    for record in RECORDS {
        journal.append(|out| out.extend_from_slice(record));
    }
    drop(journal);
    let bytes = fs::read(&path).unwrap();
    assert_eq!(read(&bytes), Some((RECORDS.to_vec(), bytes.len())));
    assert_eq!(bytes, journal_of_records());
    assert!(matches!(
        JournalFrames::new(b"campfire/journal/v0"),
        Err(JournalError::NotJournal)
    ));
}
