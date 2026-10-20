use std::sync::{Arc, Condvar};

use campfire_threads::{ThreadName, Worker, WorkerLock};

/// A worker that writes the newest value it is given: a value not yet written is replaced by a
/// newer one, so a slow disk skips values and never queues them, and no caller waits for it. A
/// failed write stops nothing, as each write is whole and the next value replaces the one that
/// failed: the newest failure is kept for the holder to take once, through `take_failure` or
/// `close`, in place of one not taken yet, as a newer value replaces an older one. Closing or
/// dropping the writer writes the value it holds, then ends the worker.
#[derive(Debug)]
pub struct LatestWriter<T, E> {
    shared: Arc<LatestShared<T, E>>,
    /// Dropped after the writer's `Drop` closes it.
    _worker: Worker,
}

/// What the caller and the worker share.
#[derive(Debug)]
struct LatestShared<T, E> {
    latest: WorkerLock<Latest<T, E>>,
    wake: Condvar,
}

#[derive(Debug)]
struct Latest<T, E> {
    /// The newest value the worker has not taken yet.
    newest: Option<T>,
    /// The newest failure the holder has not taken yet.
    failure: Option<E>,
    closing: bool,
}

impl<T: Send + 'static, E: Send + 'static> LatestWriter<T, E> {
    /// Starts the worker `name`, which writes each value with `write`.
    pub fn start(
        name: impl Into<ThreadName>,
        mut write: impl FnMut(&T) -> Result<(), E> + Send + 'static,
    ) -> Self {
        let shared = Arc::new(LatestShared {
            latest: WorkerLock::new(Latest {
                newest: None,
                failure: None,
                closing: false,
            }),
            wake: Condvar::new(),
        });
        let writing = Arc::clone(&shared);
        let worker = Worker::start(name, move || {
            loop {
                let mut latest = writing.latest.lock();
                while latest.newest.is_none() && !latest.closing {
                    latest = writing.latest.wait(&writing.wake, latest);
                }
                let Some(value) = latest.newest.take() else {
                    return;
                };
                drop(latest);
                if let Err(error) = write(&value) {
                    writing.latest.lock().failure = Some(error);
                }
            }
        });
        Self {
            shared,
            _worker: worker,
        }
    }

    /// Hands `value` to the worker, in place of one it has not taken yet.
    pub fn give(&self, value: T) {
        self.shared.latest.lock().newest = Some(value);
        self.shared.wake.notify_one();
    }

    /// The newest write that failed since the last call, if one did.
    pub fn take_failure(&self) -> Option<E> {
        self.shared.latest.lock().failure.take()
    }

    /// Closes the writer, as a drop does, once the worker wrote the value it held; the newest
    /// failure no `take_failure` took, if one.
    pub fn stop(self) -> Option<E> {
        let shared = Arc::clone(&self.shared);
        drop(self);
        shared.latest.lock().failure.take()
    }
}

impl<T, E> Drop for LatestWriter<T, E> {
    fn drop(&mut self) {
        let mut latest = self.shared.latest.lock_to_close();
        latest.closing = true;
        drop(latest);
        self.shared.wake.notify_one();
    }
}

#[cfg(test)]
mod tests;
