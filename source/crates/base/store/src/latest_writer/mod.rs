use std::mem;
use std::sync::{Arc, Condvar};

use crate::worker::Worker;
use crate::worker::worker_lock::WorkerLock;

/// A worker that writes the newest value it is given: a value not yet written is replaced by a
/// newer one, so a slow disk skips values and never queues them, and no caller waits for it.
/// Each failure is kept for the caller to take. Dropping the writer writes the value it holds,
/// then ends the worker.
#[derive(Debug)]
pub struct LatestWriter<T, E> {
    shared: Arc<LatestShared<T, E>>,
    /// Dropped after the writer's `Drop` closes it.
    _worker: Worker,
}

/// What the caller and the worker share.
#[derive(Debug)]
struct LatestShared<T, E> {
    slot: WorkerLock<Slot<T, E>>,
    wake: Condvar,
}

#[derive(Debug)]
struct Slot<T, E> {
    /// The newest value the worker has not taken yet.
    newest: Option<T>,
    /// The failures the caller has not taken yet.
    failures: Vec<E>,
    closing: bool,
}

impl<T: Send + 'static, E: Send + 'static> LatestWriter<T, E> {
    /// Starts the worker `name`, which writes each value with `write`.
    pub fn start(
        name: &str,
        mut write: impl FnMut(&T) -> Result<(), E> + Send + 'static,
    ) -> LatestWriter<T, E> {
        let shared = Arc::new(LatestShared {
            slot: WorkerLock::new(Slot {
                newest: None,
                failures: Vec::new(),
                closing: false,
            }),
            wake: Condvar::new(),
        });
        let writing = Arc::clone(&shared);
        let worker = Worker::start(name, move || {
            loop {
                let mut slot = writing.slot.lock();
                while slot.newest.is_none() && !slot.closing {
                    slot = writing.slot.wait(&writing.wake, slot);
                }
                let Some(value) = slot.newest.take() else {
                    return;
                };
                drop(slot);
                if let Err(error) = write(&value) {
                    writing.slot.lock().failures.push(error);
                }
            }
        });
        LatestWriter {
            shared,
            _worker: worker,
        }
    }

    /// Hands `value` to the worker, in place of one it has not taken yet.
    pub fn give(&self, value: T) {
        self.shared.slot.lock().newest = Some(value);
        self.shared.wake.notify_one();
    }

    /// The writes that failed since the last call.
    pub fn take_failures(&self) -> Vec<E> {
        mem::take(&mut self.shared.slot.lock().failures)
    }
}

impl<T, E> Drop for LatestWriter<T, E> {
    fn drop(&mut self) {
        let mut slot = self.shared.slot.lock_to_close();
        slot.closing = true;
        drop(slot);
        self.shared.wake.notify_one();
    }
}

#[cfg(test)]
mod tests;
