use std::sync::atomic::AtomicBool;

use polling::Poller;

/// The thread's stop: a flag the drop raises, and the poller it waits on, which the drop wakes.
#[derive(Debug)]
pub(crate) struct Stop {
    pub(crate) stopped: AtomicBool,
    pub(crate) poller: Poller,
}
