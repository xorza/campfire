use std::fs::OpenOptions;
use std::mem;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar};
use std::time::{Duration, Instant};

use crate::append_writer::append_file::AppendFile;
use crate::append_writer::append_watch::AppendWatch;
use crate::append_writer::error::{AppendError, AppendOpenError};
use crate::append_writer::slow_sync::SlowSync;
use crate::durable_file::DurableFile;
use crate::path_error::PathError;
use crate::worker::Worker;
use crate::worker::failure::Failure;
use crate::worker::worker_lock::WorkerLock;

pub(crate) mod append_file;
pub(crate) mod append_watch;
pub(crate) mod error;
pub(crate) mod slow_sync;

/// A file that records are appended to, as a write-ahead journal's: the caller puts each record
/// into a buffer, never touching the disk, and a worker appends what the buffer holds, syncs, and
/// publishes how many records are durable; the next sync waits for the next records, so a burst
/// of records shares one sync. A crash loses at most the records not yet synced. A failed write
/// or sync is never retried, as the data a failed sync held may be lost and a retry can succeed
/// over the loss: the worker stops, and reports the failure once. A sync slower than
/// `SLOW_SYNC` is kept for the caller to see. Dropping the writer writes and syncs what it holds,
/// then ends the worker.
#[derive(Debug)]
pub struct AppendWriter {
    shared: Arc<AppendShared>,
    /// Dropped after the writer's `Drop` closes it.
    _worker: Worker,
}

/// What the caller and the worker share.
#[derive(Debug)]
struct AppendShared {
    pending: WorkerLock<Pending>,
    wake: Condvar,
    durable: AtomicU64,
    failure: Failure<AppendError>,
    /// A sync longer than this is slow.
    slow_after: Duration,
    slow: WorkerLock<Option<SlowSync>>,
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
    /// How long a sync takes before it is slow, as etcd warns of a slow `fdatasync`: a sync of
    /// a few records takes milliseconds on a healthy disk, and a second is far past what a
    /// burst of them costs.
    pub const SLOW_SYNC: Duration = Duration::from_secs(1);

    /// A new file at `path`, holding `head` alone, written durably as a whole file, and open to
    /// append on the worker `name`.
    pub fn create(
        name: &str,
        path: &Path,
        head: &[u8],
    ) -> Result<AppendWriter, PathError<AppendOpenError>> {
        let open = || {
            DurableFile::write_case(path, head).map_err(AppendOpenError::Create)?;
            OpenOptions::new()
                .append(true)
                .open(path)
                .map_err(AppendOpenError::Open)
        };
        let file = open().map_err(PathError::at(path))?;
        Ok(AppendWriter::start(name, file))
    }

    /// The file at `path`, cut to its first `whole` bytes, those a read found whole, so a torn
    /// tail is gone before the next record, and open to append on the worker `name`.
    pub fn reopen(
        name: &str,
        path: &Path,
        whole: u64,
    ) -> Result<AppendWriter, PathError<AppendOpenError>> {
        let open = || {
            let file = OpenOptions::new()
                .write(true)
                .open(path)
                .map_err(AppendOpenError::Open)?;
            file.set_len(whole).map_err(AppendOpenError::Cut)?;
            file.sync_all().map_err(AppendOpenError::Cut)?;
            drop(file);
            OpenOptions::new()
                .append(true)
                .open(path)
                .map_err(AppendOpenError::Open)
        };
        let file = open().map_err(PathError::at(path))?;
        Ok(AppendWriter::start(name, file))
    }

    /// A writer that appends to `file` on the worker `name`.
    fn start<F: AppendFile>(name: &str, file: F) -> AppendWriter {
        AppendWriter::start_slow_after(name, file, AppendWriter::SLOW_SYNC)
    }

    /// A writer whose syncs longer than `slow_after` are slow.
    fn start_slow_after<F: AppendFile>(name: &str, file: F, slow_after: Duration) -> AppendWriter {
        let shared = Arc::new(AppendShared {
            pending: WorkerLock::default(),
            wake: Condvar::new(),
            durable: AtomicU64::new(0),
            failure: Failure::new(),
            slow_after,
            slow: WorkerLock::default(),
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
        let mut pending = self.shared.pending.lock();
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

impl AppendShared {
    /// The worker: takes what the caller appended, writes and syncs it, until the writer closes
    /// or a write or a sync fails.
    fn write<F: AppendFile>(&self, mut file: F) {
        let mut taken = Vec::new();
        loop {
            let mut pending = self.pending.lock();
            while pending.bytes.is_empty() && !pending.closing {
                pending = self.pending.wait(&self.wake, pending);
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
                .and_then(|()| {
                    let start = Instant::now();
                    let synced = file.sync().map_err(AppendError::Sync);
                    self.note_sync(start.elapsed());
                    synced
                });
            if let Err(error) = written {
                self.failure.keep(error);
                let mut pending = self.pending.lock();
                pending.stopped = true;
                pending.bytes = Vec::new();
                return;
            }
            self.durable.store(records, Ordering::Release);
            taken.clear();
        }
    }

    /// Keeps a sync that took `took` when it is slow, and slower than one kept.
    fn note_sync(&self, took: Duration) {
        if took <= self.slow_after {
            return;
        }
        let mut slow = self.slow.lock();
        if slow.is_none_or(|kept| kept.took < took) {
            *slow = Some(SlowSync { took });
        }
    }
}

impl Drop for AppendWriter {
    fn drop(&mut self) {
        let mut pending = self.shared.pending.lock_to_close();
        pending.closing = true;
        drop(pending);
        self.shared.wake.notify_one();
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::io;

    use crate::append_writer::AppendWriter;
    use crate::append_writer::append_file::AppendFile;

    /// A file whose appends take and whose every sync fails.
    #[derive(Debug)]
    struct FailingSync;

    impl AppendFile for FailingSync {
        fn append(&mut self, _: &[u8]) -> io::Result<()> {
            Ok(())
        }

        fn sync(&mut self) -> io::Result<()> {
            Err(io::Error::other("sync failed"))
        }
    }

    impl AppendWriter {
        /// A writer on the worker `name` whose every sync fails, as a full or a failing disk's.
        pub fn failing_sync(name: &str) -> AppendWriter {
            AppendWriter::start(name, FailingSync)
        }
    }
}

#[cfg(test)]
mod tests;
