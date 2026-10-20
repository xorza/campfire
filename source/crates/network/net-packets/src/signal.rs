use core::time::Duration;
use std::sync::{Condvar, Mutex};
use std::task::Wake;

/// A flag a thread waits on, which another sets: a wake that comes before the wait is kept, so
/// none is lost.
#[derive(Debug, Default)]
pub struct Signal {
    raised: Mutex<bool>,
    condvar: Condvar,
}

impl Signal {
    /// Waits until the flag is raised or `timeout` passes, and lowers it.
    pub fn wait(&self, timeout: Option<Duration>) {
        let raised = self
            .raised
            .lock()
            .expect("no thread panics holding a signal");
        let mut raised = match timeout {
            Some(timeout) => {
                self.condvar
                    .wait_timeout_while(raised, timeout, |raised| !*raised)
                    .expect("no thread panics holding a signal")
                    .0
            }
            None => self
                .condvar
                .wait_while(raised, |raised| !*raised)
                .expect("no thread panics holding a signal"),
        };
        *raised = false;
    }

    pub fn raise(&self) {
        *self
            .raised
            .lock()
            .expect("no thread panics holding a signal") = true;
        self.condvar.notify_one();
    }
}

impl Wake for Signal {
    fn wake(self: std::sync::Arc<Self>) {
        self.raise();
    }

    fn wake_by_ref(self: &std::sync::Arc<Self>) {
        self.raise();
    }
}

#[cfg(test)]
mod tests {
    use campfire_store::Worker;

    use super::*;

    impl Signal {
        /// Whether the flag is raised, which it then lowers.
        pub(crate) fn take_raised(&self) -> bool {
            core::mem::replace(&mut *self.raised.lock().unwrap(), false)
        }
    }
    use std::sync::Arc;
    use std::time::Instant;

    /// A raise before the wait ends it at once, and a raise from another thread ends one that
    /// waits with no timeout; a wait with a timeout and no raise ends at the timeout.
    #[test]
    fn a_raise_ends_a_wait_and_none_is_lost() {
        let signal = Arc::new(Signal::default());
        signal.raise();
        signal.wait(None);
        let raiser = Arc::clone(&signal);
        let worker = Worker::start("raiser", move || raiser.raise());
        signal.wait(None);
        drop(worker);
        let start = Instant::now();
        signal.wait(Some(Duration::from_millis(5)));
        assert!(start.elapsed() >= Duration::from_millis(5));
    }
}
