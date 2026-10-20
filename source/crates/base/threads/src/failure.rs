use crate::worker_lock::WorkerLock;

/// The failure that stopped a worker, kept until its holder takes it, once.
#[derive(Debug)]
pub struct Failure<E>(WorkerLock<Option<E>>);

impl<E> Failure<E> {
    pub const fn new() -> Self {
        Self(WorkerLock::new(None))
    }

    /// Keeps `error`, the one failure that stopped the worker.
    pub fn keep(&self, error: E) {
        let mut kept = self.0.lock();
        debug_assert!(kept.is_none(), "a worker stops at its first failure");
        *kept = Some(error);
    }

    /// The failure, the first time it is asked after it; none before it, and after.
    pub fn take(&self) -> Option<E> {
        self.0.lock().take()
    }
}

impl<E> Default for Failure<E> {
    fn default() -> Self {
        Self::new()
    }
}
