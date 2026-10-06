use std::mem;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};

use crate::worker::Worker;

/// A worker that writes the newest value it is given: a value not yet written is replaced by a
/// newer one, so a slow disk skips values and never queues them, and no caller waits for it.
/// Each failure is kept for the caller to take. Dropping the writer writes the value it holds,
/// then ends the worker.
#[derive(Debug)]
pub struct LatestWriter<T, E> {
    shared: Arc<Shared<T, E>>,
    /// Dropped after the writer's `Drop` closes it.
    _worker: Worker,
}

/// What the caller and the worker share.
#[derive(Debug)]
struct Shared<T, E> {
    slot: Mutex<Slot<T, E>>,
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
        let shared = Arc::new(Shared {
            slot: Mutex::new(Slot {
                newest: None,
                failures: Vec::new(),
                closing: false,
            }),
            wake: Condvar::new(),
        });
        let writing = Arc::clone(&shared);
        let worker = Worker::start(name, move || {
            loop {
                let mut slot = writing.lock();
                while slot.newest.is_none() && !slot.closing {
                    slot = writing
                        .wake
                        .wait(slot)
                        .expect("no thread panics holding the slot");
                }
                let Some(value) = slot.newest.take() else {
                    return;
                };
                drop(slot);
                if let Err(error) = write(&value) {
                    writing.lock().failures.push(error);
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
        self.shared.lock().newest = Some(value);
        self.shared.wake.notify_one();
    }

    /// The writes that failed since the last call.
    pub fn take_failures(&self) -> Vec<E> {
        mem::take(&mut self.shared.lock().failures)
    }
}

impl<T, E> Shared<T, E> {
    fn lock(&self) -> MutexGuard<'_, Slot<T, E>> {
        self.slot.lock().expect("no thread panics holding the slot")
    }
}

impl<T, E> Drop for LatestWriter<T, E> {
    fn drop(&mut self) {
        // A drop as a panic unwinds must not panic again, which would abort.
        let mut slot = self
            .shared
            .slot
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        slot.closing = true;
        drop(slot);
        self.shared.wake.notify_one();
    }
}

#[cfg(test)]
mod tests;
