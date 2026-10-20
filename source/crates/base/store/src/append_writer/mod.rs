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
use campfire_threads::{Failure, ThreadName, Worker, WorkerLock};

pub(crate) mod append_file;
pub(crate) mod append_watch;
pub(crate) mod error;
pub(crate) mod slow_sync;

/// A file that records are appended to, as a write-ahead journal's: the caller puts each record
/// into a buffer, never touching the disk, and a worker appends what the buffer holds, syncs, and
/// publishes how many records are durable; the next sync waits for the next records, so a burst
/// of records shares one sync. A crash loses at most the records not yet synced. A failed write
/// or sync is never retried, as the data a failed sync held may be lost and a retry can succeed
/// over the loss: the worker stops, and keeps the failure for its holder to take once, through
/// the writer's watch or its `close`. A sync slower than `SLOW_SYNC` is kept for the caller to
/// see. The writer fails, as a failed sync does, when a write and its sync run past `HUNG_SYNC`
/// as the watch sees it, or when the caller's records the worker has not taken pass
/// `MAX_BEHIND`, so a disk that stops or falls behind ends the writer rather than grows its
/// memory without a bound. Closing or dropping the writer writes and syncs what it holds, then
/// ends the worker.
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
    bounds: AppendBounds,
    slow: WorkerLock<Option<SlowSync>>,
    /// When the write and the sync that run now began.
    running: WorkerLock<Option<Instant>>,
}

/// The bounds a writer keeps: `AppendWriter::BOUNDS`, or shorter ones under test.
#[derive(Debug, Clone, Copy)]
struct AppendBounds {
    /// A sync longer than this is slow.
    slow_after: Duration,
    /// A write and its sync that run longer than this are hung.
    hung_after: Duration,
    /// The most bytes of records the worker has not taken.
    max_behind: usize,
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

    /// How long a write and its sync run before they are hung: two minutes, as long as Linux's
    /// hung task detector lets a task wait on the disk before it reports it.
    pub const HUNG_SYNC: Duration = Duration::from_secs(120);

    /// The most bytes of records the worker has not taken: 64 MiB, minutes of a busy match's
    /// journal at tens of kilobytes a second, so only a disk far behind its writer comes to it.
    pub const MAX_BEHIND: usize = 64 << 20;

    const BOUNDS: AppendBounds = AppendBounds {
        slow_after: Self::SLOW_SYNC,
        hung_after: Self::HUNG_SYNC,
        max_behind: Self::MAX_BEHIND,
    };

    /// A new file at `path`, holding `head` alone, written durably as a whole file, and open to
    /// append on the worker `name`.
    pub fn create(
        name: impl Into<ThreadName>,
        path: &Path,
        head: &[u8],
    ) -> Result<Self, PathError<AppendOpenError>> {
        let open = || {
            DurableFile::write_case(path, head).map_err(AppendOpenError::Create)?;
            OpenOptions::new()
                .append(true)
                .open(path)
                .map_err(AppendOpenError::Open)
        };
        let file = open().map_err(PathError::at(path))?;
        Ok(Self::start(name.into(), file))
    }

    /// The file at `path`, cut to its first `whole` bytes, those a read found whole, so a torn
    /// tail is gone before the next record, and open to append on the worker `name`.
    pub fn reopen(
        name: impl Into<ThreadName>,
        path: &Path,
        whole: u64,
    ) -> Result<Self, PathError<AppendOpenError>> {
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
        Ok(Self::start(name.into(), file))
    }

    /// A writer that appends to `file` on the worker `name`.
    fn start<F: AppendFile>(name: ThreadName, file: F) -> Self {
        Self::start_bounded(name, file, Self::BOUNDS)
    }

    /// A writer that keeps `bounds`.
    fn start_bounded<F: AppendFile>(name: ThreadName, file: F, bounds: AppendBounds) -> Self {
        let shared = Arc::new(AppendShared {
            pending: WorkerLock::default(),
            wake: Condvar::new(),
            durable: AtomicU64::new(0),
            failure: Failure::new(),
            bounds,
            slow: WorkerLock::default(),
            running: WorkerLock::default(),
        });
        let writing = Arc::clone(&shared);
        let worker = Worker::start(name, move || writing.write(file));
        Self {
            shared,
            _worker: worker,
        }
    }

    /// Appends the `records` records `write` puts in the buffer it is given, at its end, for the
    /// worker, which one wake hands it; nothing once the worker stopped at a failure. Records that
    /// take the buffer past `MAX_BEHIND` stop the writer.
    pub fn append(&self, records: u64, write: impl FnOnce(&mut Vec<u8>)) {
        let mut pending = self.shared.pending.lock();
        if pending.stopped {
            return;
        }
        write(&mut pending.bytes);
        pending.records += records;
        let bytes = pending.bytes.len();
        if bytes > self.shared.bounds.max_behind {
            self.shared
                .stop(&mut pending, AppendError::Behind { bytes });
            return;
        }
        drop(pending);
        self.shared.wake.notify_one();
    }

    pub fn watch(&self) -> AppendWatch {
        AppendWatch(Arc::clone(&self.shared))
    }

    /// Closes the writer, as a drop does, once the worker wrote and synced what it held; the
    /// failure that stopped it, if one did and no `take_failure` took it.
    pub fn close(self) -> Option<AppendError> {
        let shared = Arc::clone(&self.shared);
        drop(self);
        shared.failure.take()
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
            *self.running.lock() = Some(Instant::now());
            let written = file
                .append(&taken)
                .map_err(AppendError::Write)
                .and_then(|()| {
                    let start = Instant::now();
                    let synced = file.sync().map_err(AppendError::Sync);
                    self.note_sync(start.elapsed());
                    synced
                });
            *self.running.lock() = None;
            if let Err(error) = written {
                self.stop(&mut self.pending.lock(), error);
                return;
            }
            self.durable.store(records, Ordering::Release);
            taken.clear();
        }
    }

    /// Stops the writer at `error`, with `pending` its records held, unless it stopped at a failure
    /// before: it keeps no record more, and keeps the failure for its holder. The failure is kept
    /// before the lock goes, so a holder that sees the writer settled finds it.
    fn stop(&self, pending: &mut Pending, error: AppendError) {
        if pending.stopped {
            return;
        }
        pending.stopped = true;
        pending.bytes = Vec::new();
        self.failure.keep(error);
    }

    /// Stops the writer when the write and the sync that run now run past `hung_after`.
    fn check_hung(&self) {
        let Some(since) = *self.running.lock() else {
            return;
        };
        let running = since.elapsed();
        if running > self.bounds.hung_after {
            self.stop(&mut self.pending.lock(), AppendError::Hung { running });
        }
    }

    /// Keeps a sync that took `took` when it is slow, and slower than one kept.
    fn note_sync(&self, took: Duration) {
        if took <= self.bounds.slow_after {
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

    use campfire_threads::ThreadName;

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
        pub fn failing_sync(name: impl Into<ThreadName>) -> Self {
            Self::start(name.into(), FailingSync)
        }
    }
}

#[cfg(test)]
mod tests;
