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
    pub fn is_settled(&self) -> bool {
        let pending = self.0.pending.lock();
        pending.stopped || self.durable() == pending.records
    }

    /// The failure that stopped the writer, the first time it is asked after the failure; none
    /// before it, and after. A write and its sync that run past `AppendWriter::HUNG_SYNC` as it
    /// is asked stop the writer, so a holder that asks each frame sees a hung disk.
    pub fn take_failure(&self) -> Option<AppendError> {
        self.0.check_hung();
        self.0.failure.take()
    }

    /// The slowest sync since the last call, when one was slow.
    pub fn take_slow_sync(&self) -> Option<SlowSync> {
        self.0.slow.lock().take()
    }
}
