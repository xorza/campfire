use std::fs::File;
use std::io::{self, Write};
use std::mem;
use std::path::Path;
use std::sync::{Arc, Condvar};

use crate::path_error::PathError;
use crate::stream_writer::error::StreamError;
use crate::stream_writer::stream_sender::StreamSender;
use crate::worker::Worker;
use crate::worker::failure::Failure;
use crate::worker::worker_lock::WorkerLock;

pub(crate) mod error;
pub(crate) mod stream_sender;

/// A file that a worker writes a stream of bytes to, in the order they come, with no sync, as a
/// log file needs: the caller puts each piece into a buffer, and the worker swaps it for its own
/// and writes it, so a caller never waits for the disk. A caller waits while the buffer holds
/// `bound` bytes or more, so no piece is lost and the buffer holds less than `bound` bytes and
/// one piece. A failed write or flush is not retried: the worker stops, keeps the failure for
/// the owner to take once, and the pieces after it are dropped. Dropping the writer writes what
/// the buffer holds, then ends the worker; a piece put after the drop began is dropped, so
/// callers that go on putting never keep it running. A crash loses what the buffer held.
#[derive(Debug)]
pub struct StreamWriter {
    shared: Arc<StreamShared>,
    /// Dropped after the writer's `Drop` closes it.
    _worker: Worker,
}

/// What the callers and the worker share.
#[derive(Debug)]
struct StreamShared {
    pending: WorkerLock<Pending>,
    /// The worker waits on it for bytes or for the close.
    filled: Condvar,
    /// A caller waits on it for room.
    emptied: Condvar,
    bound: usize,
    failure: Failure<StreamError>,
}

/// The bytes the callers put and the worker has not taken yet.
#[derive(Debug, Default)]
struct Pending {
    bytes: Vec<u8>,
    /// The writer dropped: the worker writes what the buffer holds, and no piece is put any more.
    closing: bool,
    /// The worker stopped at a failure, so no piece is kept any more.
    failed: bool,
}

impl StreamWriter {
    /// A new file at `path`, in place of one there, written on the worker `name`, with a buffer
    /// of `bound` bytes.
    pub fn create(
        name: &str,
        path: &Path,
        bound: usize,
    ) -> Result<StreamWriter, PathError<io::Error>> {
        let file = File::create(path).map_err(PathError::at(path))?;
        Ok(StreamWriter::start(name, file, bound))
    }

    /// A writer that writes to `file` on the worker `name`, with a buffer of `bound` bytes.
    fn start<W: Write + Send + 'static>(name: &str, file: W, bound: usize) -> StreamWriter {
        assert!(bound > 0, "a stream's buffer holds a byte");
        let shared = Arc::new(StreamShared {
            pending: WorkerLock::default(),
            filled: Condvar::new(),
            emptied: Condvar::new(),
            bound,
            failure: Failure::new(),
        });
        let writing = Arc::clone(&shared);
        let worker = Worker::start(name, move || writing.write(file));
        StreamWriter {
            shared,
            _worker: worker,
        }
    }

    /// A handle that puts pieces into the writer's buffer, for a caller that cannot hold the
    /// writer, as a log layer does: once the writer drops, its pieces are dropped.
    pub fn sender(&self) -> StreamSender {
        StreamSender(Arc::clone(&self.shared))
    }

    /// The write or flush that stopped the worker, the first time it is asked after the
    /// failure; none before it, and after.
    pub fn take_failure(&self) -> Option<StreamError> {
        self.shared.failure.take()
    }

    /// Closes the stream, as a drop does, once the worker wrote what the buffer held; the
    /// failure that stopped it, if one did and no `take_failure` took it.
    pub fn close(self) -> Option<StreamError> {
        let shared = Arc::clone(&self.shared);
        drop(self);
        shared.failure.take()
    }
}

impl StreamShared {
    /// Puts the piece `put` writes at the end of the buffer, once it has room; nothing once the
    /// writer drops or the worker stopped at a failure.
    fn put(&self, put: impl FnOnce(&mut Vec<u8>)) {
        let mut pending = self.pending.lock();
        while !pending.closing && !pending.failed && pending.bytes.len() >= self.bound {
            pending = self.pending.wait(&self.emptied, pending);
        }
        if pending.closing || pending.failed {
            return;
        }
        put(&mut pending.bytes);
        drop(pending);
        self.filled.notify_one();
    }

    /// The worker: takes what the callers put and writes it, until the writer closes and the
    /// buffer is empty, or a write fails.
    fn write<W: Write>(&self, mut file: W) {
        let mut taken = Vec::new();
        loop {
            let mut pending = self.pending.lock();
            while pending.bytes.is_empty() && !pending.closing {
                pending = self.pending.wait(&self.filled, pending);
            }
            if pending.bytes.is_empty() {
                return;
            }
            taken.clear();
            mem::swap(&mut taken, &mut pending.bytes);
            drop(pending);
            self.emptied.notify_all();
            let written = file
                .write_all(&taken)
                .map_err(StreamError::Write)
                .and_then(|()| file.flush().map_err(StreamError::Flush));
            if let Err(error) = written {
                self.failure.keep(error);
                let mut pending = self.pending.lock();
                pending.failed = true;
                pending.bytes.clear();
                drop(pending);
                self.emptied.notify_all();
                return;
            }
        }
    }
}

/// Closes the stream: the worker writes what the buffer holds, then ends, and the writer's
/// worker joins it as it drops after this. A caller waiting for room puts nothing.
impl Drop for StreamWriter {
    fn drop(&mut self) {
        let mut pending = self.shared.pending.lock_to_close();
        pending.closing = true;
        drop(pending);
        self.shared.filled.notify_one();
        self.shared.emptied.notify_all();
    }
}

#[cfg(test)]
mod tests;
