use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::append_writer::AppendShared;
use crate::append_writer::error::AppendError;
use crate::append_writer::slow_sync::SlowSync;

/// What another part of the program sees of an append writer: how many of its records are
/// durable, and its failure.
#[derive(Debug, Clone)]
pub struct AppendWatch(pub(super) Arc<AppendShared>);

impl AppendWatch {
    /// How many records the writer synced.
    pub fn durable(&self) -> u64 {
        self.0.durable.load(Ordering::Acquire)
    }

    /// Whether the writer synced every record appended so far, or stopped at a failure, after
    /// which it keeps none.
    pub fn settled(&self) -> bool {
        let pending = self.0.lock();
        pending.stopped || self.durable() == pending.records
    }

    /// The write or sync that failed, the first time it is asked after the failure; none
    /// before it, and after.
    pub fn take_failure(&self) -> Option<AppendError> {
        self.0
            .failure
            .lock()
            .expect("no thread panics holding the failure")
            .take()
    }

    /// The slowest sync since the last call, when one was slow.
    pub fn take_slow_sync(&self) -> Option<SlowSync> {
        self.0
            .slow
            .lock()
            .expect("no thread panics holding a slow sync")
            .take()
    }
}
