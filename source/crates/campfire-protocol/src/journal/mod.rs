use std::fs::OpenOptions;
use std::io;
use std::mem;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};

use crate::durable_file::DurableFile;
use crate::journal::error::JournalError;
use crate::journal::journal_file::JournalFile;
use crate::journal::journal_frames::{JOURNAL_TAG, JournalFrames};
use crate::journal::journal_watch::JournalWatch;

pub(crate) mod error;
pub(crate) mod journal_file;
pub(crate) mod journal_frames;
pub(crate) mod journal_watch;

/// A session's private write-ahead journal: the main thread frames each record into a buffer,
/// never touching the disk, and a writer thread appends what the buffer holds, syncs, and
/// publishes how many records are durable; the next sync waits for the next records, so a busy
/// tick groups many records in one sync. A crash loses at most the records not yet synced. A
/// failed write or sync is never retried, as the data a failed sync held may be lost and a retry
/// can succeed over the loss: the writer stops, and reports the failure once. Dropping the
/// journal writes and syncs what it holds, then ends the thread.
#[derive(Debug)]
pub struct Journal {
    shared: Arc<Shared>,
    writer: Option<JoinHandle<()>>,
}

/// What the main thread and the writer share.
#[derive(Debug)]
pub(crate) struct Shared {
    pending: Mutex<Pending>,
    wake: Condvar,
    pub(crate) durable: AtomicU64,
    pub(crate) failure: Mutex<Option<io::Error>>,
}

/// The frames the main thread appended and the writer has not taken yet.
#[derive(Debug, Default)]
struct Pending {
    bytes: Vec<u8>,
    /// The records appended since the start, taken or not.
    records: u64,
    closing: bool,
    /// The writer stopped at a failure, so no record is kept any more.
    stopped: bool,
}

impl Journal {
    /// A new journal at `path`, holding its tag alone, written durably as a whole file, and open
    /// to append.
    pub fn create(path: &Path) -> Result<Journal, JournalError> {
        DurableFile::write(path, JOURNAL_TAG).map_err(JournalError::Create)?;
        let file = OpenOptions::new()
            .append(true)
            .open(path)
            .map_err(JournalError::Open)?;
        Ok(Journal::start(file))
    }

    /// The journal at `path`, cut to its first `whole` bytes, the tag and the frames a read found
    /// whole, so a torn tail is gone before the next record, and open to append.
    pub fn reopen(path: &Path, whole: u64) -> Result<Journal, JournalError> {
        let file = OpenOptions::new()
            .write(true)
            .open(path)
            .map_err(JournalError::Open)?;
        file.set_len(whole).map_err(JournalError::Cut)?;
        file.sync_all().map_err(JournalError::Cut)?;
        drop(file);
        let file = OpenOptions::new()
            .append(true)
            .open(path)
            .map_err(JournalError::Open)?;
        Ok(Journal::start(file))
    }

    /// A journal that appends to `file`, which holds the tag and every record before.
    pub fn start<F: JournalFile>(file: F) -> Journal {
        let shared = Arc::new(Shared {
            pending: Mutex::new(Pending::default()),
            wake: Condvar::new(),
            durable: AtomicU64::new(0),
            failure: Mutex::new(None),
        });
        let writing = Arc::clone(&shared);
        let writer = thread::Builder::new()
            .name("journal".to_owned())
            .spawn(move || Journal::write(&writing, file))
            .expect("the OS starts a thread");
        Journal {
            shared,
            writer: Some(writer),
        }
    }

    /// Frames the record `write` puts in the buffer it is given, at its end, for the writer.
    pub(crate) fn append(&self, write: impl FnOnce(&mut Vec<u8>)) {
        let mut pending = self.lock();
        if pending.stopped {
            return;
        }
        let start = JournalFrames::open(&mut pending.bytes);
        write(&mut pending.bytes);
        JournalFrames::seal(&mut pending.bytes, start);
        pending.records += 1;
        drop(pending);
        self.shared.wake.notify_one();
    }

    pub fn watch(&self) -> JournalWatch {
        JournalWatch(Arc::clone(&self.shared))
    }

    fn lock(&self) -> MutexGuard<'_, Pending> {
        self.shared
            .pending
            .lock()
            .expect("the journal's writer does not panic")
    }

    /// The writer thread: takes what the main thread appended, writes and syncs it, until the
    /// journal closes or a write or a sync fails.
    fn write<F: JournalFile>(shared: &Shared, mut file: F) {
        let mut taken = Vec::new();
        loop {
            let mut pending = shared
                .pending
                .lock()
                .expect("the main thread does not panic holding the journal");
            while pending.bytes.is_empty() && !pending.closing {
                pending = shared
                    .wake
                    .wait(pending)
                    .expect("the main thread does not panic holding the journal");
            }
            if pending.bytes.is_empty() {
                return;
            }
            mem::swap(&mut pending.bytes, &mut taken);
            let records = pending.records;
            drop(pending);
            if let Err(error) = file.append(&taken).and_then(|()| file.sync()) {
                *shared
                    .failure
                    .lock()
                    .expect("no thread panics holding the failure") = Some(error);
                let mut pending = shared
                    .pending
                    .lock()
                    .expect("the main thread does not panic holding the journal");
                pending.stopped = true;
                pending.bytes = Vec::new();
                return;
            }
            shared.durable.store(records, Ordering::Release);
            taken.clear();
        }
    }
}

impl Drop for Journal {
    fn drop(&mut self) {
        self.lock().closing = true;
        self.shared.wake.notify_one();
        if let Some(writer) = self.writer.take() {
            writer.join().expect("the journal's writer does not panic");
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;
