use std::io;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::journal::Shared;

/// What another part of the server sees of a journal: how many of its records are durable, and
/// its failure.
#[derive(Debug, Clone)]
pub struct JournalWatch(pub(crate) Arc<Shared>);

impl JournalWatch {
    /// How many records the writer synced.
    pub fn durable(&self) -> u64 {
        self.0.durable.load(Ordering::Acquire)
    }

    /// The write or sync that failed, the first time it is asked after the failure; none
    /// before it, and after.
    pub fn take_failure(&self) -> Option<io::Error> {
        self.0
            .failure
            .lock()
            .expect("the journal's writer does not panic")
            .take()
    }
}
