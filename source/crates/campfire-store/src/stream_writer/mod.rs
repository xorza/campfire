use std::fs::File;
use std::io::{self, Write};
use std::mem;
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

use crate::stream_writer::stream_sender::StreamSender;
use crate::worker::Worker;

pub(crate) mod stream_sender;

/// A file that a worker writes a stream of bytes to, in the order they come, with no sync, as a
/// log file needs: the caller puts each piece into a buffer, and the worker swaps it for its own
/// and writes it, so a caller never waits for the disk. The buffer holds at most `bound` bytes,
/// and a caller waits while it is full, so no piece is lost and memory stays bounded; one piece
/// longer than the bound goes into an empty buffer. A failed write is not retried: the worker
/// stops, keeps the failure, and the pieces after it are dropped. Dropping the writer writes
/// what it holds, then ends the worker; a crash loses what it held.
#[derive(Debug)]
pub struct StreamWriter {
    shared: Arc<StreamShared>,
    /// Dropped after the writer's `Drop` closes it.
    _worker: Worker,
}

/// What the callers and the worker share.
#[derive(Debug)]
struct StreamShared {
    pending: Mutex<Pending>,
    /// The worker waits on it for bytes or for the close.
    filled: Condvar,
    /// A caller waits on it for room.
    emptied: Condvar,
    bound: usize,
    failure: Mutex<Option<io::Error>>,
}

/// The bytes the callers put and the worker has not taken yet.
#[derive(Debug, Default)]
struct Pending {
    bytes: Vec<u8>,
    closing: bool,
    /// The worker stopped at a failure, or the writer closed, so no piece is kept any more.
    stopped: bool,
}

impl StreamWriter {
    /// A new file at `path`, in place of one there, written on the worker `name`, with a buffer
    /// of `bound` bytes.
    pub fn create(name: &str, path: &Path, bound: usize) -> io::Result<StreamWriter> {
        File::create(path).map(|file| StreamWriter::start(name, file, bound))
    }

    /// A writer that writes to `file` on the worker `name`, with a buffer of `bound` bytes.
    pub fn start<W: Write + Send + 'static>(name: &str, file: W, bound: usize) -> StreamWriter {
        assert!(bound > 0, "a stream's buffer holds a byte");
        let shared = Arc::new(StreamShared {
            pending: Mutex::new(Pending::default()),
            filled: Condvar::new(),
            emptied: Condvar::new(),
            bound,
            failure: Mutex::new(None),
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

    /// Closes the stream, as a drop does, once the worker wrote what the buffer held; the
    /// failure that stopped it, if one did.
    pub fn close(self) -> Option<io::Error> {
        let shared = Arc::clone(&self.shared);
        drop(self);
        shared
            .failure
            .lock()
            .expect("no thread panics holding a stream's failure")
            .take()
    }

    /// The failure that stopped the worker, once; none while it writes.
    pub fn take_failure(&self) -> Option<io::Error> {
        self.shared
            .failure
            .lock()
            .expect("no thread panics holding a stream's failure")
            .take()
    }
}

impl StreamShared {
    fn lock(&self) -> MutexGuard<'_, Pending> {
        self.pending
            .lock()
            .expect("no thread panics holding a stream's bytes")
    }

    /// Puts the piece `put` writes at the end of the buffer, once it has room; nothing once the
    /// stream stopped.
    fn put(&self, put: impl FnOnce(&mut Vec<u8>)) {
        let mut pending = self.lock();
        while !pending.stopped && pending.bytes.len() >= self.bound {
            pending = self
                .emptied
                .wait(pending)
                .expect("no thread panics holding a stream's bytes");
        }
        if pending.stopped {
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
            let mut pending = self.lock();
            while pending.bytes.is_empty() && !pending.closing {
                pending = self
                    .filled
                    .wait(pending)
                    .expect("no thread panics holding a stream's bytes");
            }
            if pending.bytes.is_empty() {
                // Closed: a piece put after this has no worker to write it.
                pending.stopped = true;
                drop(pending);
                self.emptied.notify_all();
                return;
            }
            taken.clear();
            mem::swap(&mut taken, &mut pending.bytes);
            drop(pending);
            self.emptied.notify_all();
            if let Err(error) = file.write_all(&taken).and_then(|()| file.flush()) {
                *self
                    .failure
                    .lock()
                    .expect("no thread panics holding a stream's failure") = Some(error);
                let mut pending = self.lock();
                pending.stopped = true;
                pending.bytes.clear();
                drop(pending);
                self.emptied.notify_all();
                return;
            }
        }
    }
}

/// Closes the stream: the worker writes what the buffer holds, then ends, and the writer's
/// worker joins it as it drops after this.
impl Drop for StreamWriter {
    fn drop(&mut self) {
        let mut pending = self.shared.lock();
        pending.closing = true;
        drop(pending);
        self.shared.filled.notify_one();
    }
}

#[cfg(test)]
mod tests;
