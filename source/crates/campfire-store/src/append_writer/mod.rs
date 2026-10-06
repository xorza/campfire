use std::fs::OpenOptions;
use std::mem;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

use crate::append_writer::append_file::AppendFile;
use crate::append_writer::append_watch::AppendWatch;
use crate::append_writer::error::{AppendError, AppendOpenError};
use crate::durable_file::DurableFile;
use crate::worker::Worker;

pub(crate) mod append_file;
pub(crate) mod append_watch;
pub(crate) mod error;

/// A file that records are appended to, as a write-ahead journal's: the caller puts each record
/// into a buffer, never touching the disk, and a worker appends what the buffer holds, syncs, and
/// publishes how many records are durable; the next sync waits for the next records, so a burst
/// of records shares one sync. A crash loses at most the records not yet synced. A failed write
/// or sync is never retried, as the data a failed sync held may be lost and a retry can succeed
/// over the loss: the worker stops, and reports the failure once. Dropping the writer writes and
/// syncs what it holds, then ends the worker.
#[derive(Debug)]
pub struct AppendWriter {
    shared: Arc<Shared>,
    /// Dropped after the writer's `Drop` closes it.
    _worker: Worker,
}

/// What the caller and the worker share.
#[derive(Debug)]
pub(crate) struct Shared {
    pending: Mutex<Pending>,
    wake: Condvar,
    durable: AtomicU64,
    failure: Mutex<Option<AppendError>>,
}

/// The records the caller appended and the worker has not taken yet.
#[derive(Debug, Default)]
struct Pending {
    bytes: Vec<u8>,
    /// The records appended since the start, taken or not.
    records: u64,
    closing: bool,
    /// The worker stopped at a failure, so no record is kept any more.
    stopped: bool,
}

impl AppendWriter {
    /// A new file at `path`, holding `head` alone, written durably as a whole file, and open to
    /// append on the worker `name`.
    pub fn create(name: &str, path: &Path, head: &[u8]) -> Result<AppendWriter, AppendOpenError> {
        DurableFile::write(path, head).map_err(AppendOpenError::Create)?;
        let file = OpenOptions::new()
            .append(true)
            .open(path)
            .map_err(AppendOpenError::Open)?;
        Ok(AppendWriter::start(name, file))
    }

    /// The file at `path`, cut to its first `whole` bytes, those a read found whole, so a torn
    /// tail is gone before the next record, and open to append on the worker `name`.
    pub fn reopen(name: &str, path: &Path, whole: u64) -> Result<AppendWriter, AppendOpenError> {
        let file = OpenOptions::new()
            .write(true)
            .open(path)
            .map_err(AppendOpenError::Open)?;
        file.set_len(whole).map_err(AppendOpenError::Cut)?;
        file.sync_all().map_err(AppendOpenError::Cut)?;
        drop(file);
        let file = OpenOptions::new()
            .append(true)
            .open(path)
            .map_err(AppendOpenError::Open)?;
        Ok(AppendWriter::start(name, file))
    }

    /// A writer that appends to `file` on the worker `name`.
    pub fn start<F: AppendFile>(name: &str, file: F) -> AppendWriter {
        let shared = Arc::new(Shared {
            pending: Mutex::new(Pending::default()),
            wake: Condvar::new(),
            durable: AtomicU64::new(0),
            failure: Mutex::new(None),
        });
        let writing = Arc::clone(&shared);
        let worker = Worker::start(name, move || writing.write(file));
        AppendWriter {
            shared,
            _worker: worker,
        }
    }

    /// Appends the record `write` puts in the buffer it is given, at its end, for the worker;
    /// nothing once the worker stopped at a failure.
    pub fn append(&self, write: impl FnOnce(&mut Vec<u8>)) {
        let mut pending = self.shared.lock();
        if pending.stopped {
            return;
        }
        write(&mut pending.bytes);
        pending.records += 1;
        drop(pending);
        self.shared.wake.notify_one();
    }

    pub fn watch(&self) -> AppendWatch {
        AppendWatch(Arc::clone(&self.shared))
    }
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Pending> {
        self.pending
            .lock()
            .expect("no thread panics holding an append writer's records")
    }

    /// The worker: takes what the caller appended, writes and syncs it, until the writer closes
    /// or a write or a sync fails.
    fn write<F: AppendFile>(&self, mut file: F) {
        let mut taken = Vec::new();
        loop {
            let mut pending = self.lock();
            while pending.bytes.is_empty() && !pending.closing {
                pending = self
                    .wake
                    .wait(pending)
                    .expect("no thread panics holding an append writer's records");
            }
            if pending.bytes.is_empty() {
                return;
            }
            mem::swap(&mut pending.bytes, &mut taken);
            let records = pending.records;
            drop(pending);
            let written = file
                .append(&taken)
                .map_err(AppendError::Write)
                .and_then(|()| file.sync().map_err(AppendError::Sync));
            if let Err(error) = written {
                *self
                    .failure
                    .lock()
                    .expect("no thread panics holding the failure") = Some(error);
                let mut pending = self.lock();
                pending.stopped = true;
                pending.bytes = Vec::new();
                return;
            }
            self.durable.store(records, Ordering::Release);
            taken.clear();
        }
    }
}

impl Drop for AppendWriter {
    fn drop(&mut self) {
        self.shared.lock().closing = true;
        self.shared.wake.notify_one();
    }
}

#[cfg(test)]
mod tests;
