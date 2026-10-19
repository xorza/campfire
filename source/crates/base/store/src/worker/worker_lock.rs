use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

/// The state a worker's holder and its thread share. A caller's code that panics as it holds the
/// state, as a piece it writes into a buffer, poisons it: the next lock of the worker or of the
/// holder then panics, rather than read what the panic left torn, and only the holder's drop reads
/// it still, to close, as a drop while a panic unwinds must not panic again, which would abort.
#[derive(Debug, Default)]
pub(crate) struct WorkerLock<S>(Mutex<S>);

/// Why a lock fails: a panic as the state was held.
const POISONED: &str = "no code panics holding a worker's state";

impl<S> WorkerLock<S> {
    pub(crate) const fn new(state: S) -> Self {
        Self(Mutex::new(state))
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, S> {
        self.0.lock().expect(POISONED)
    }

    /// Waits on `wake` with `held`, the guard of this lock, as `Condvar::wait` does.
    pub(crate) fn wait<'a>(&self, wake: &Condvar, held: MutexGuard<'a, S>) -> MutexGuard<'a, S> {
        wake.wait(held).expect(POISONED)
    }

    /// The state, poisoned or not, for the holder's drop to close.
    pub(crate) fn lock_to_close(&self) -> MutexGuard<'_, S> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn get_mut(&mut self) -> &mut S {
        self.0.get_mut().expect(POISONED)
    }
}
