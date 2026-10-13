use std::time::Duration;

/// A sync of an append writer that took longer than `AppendWriter::SLOW_SYNC`: the slowest since
/// the last one taken. It loses nothing, as a sync that returns late is still a sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlowSync {
    pub took: Duration,
}
