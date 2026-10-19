use std::fmt::Debug;

/// Where a session log's journal records go as the log takes them: a file's writer on a server,
/// a buffer under test. The log does no IO itself, and moves to the threads its owner runs on.
pub trait RecordSink: Debug + Send + Sync {
    /// Takes `records` framed records, which `write` puts at the end of the buffer it is given.
    fn append(&self, records: u64, write: &mut dyn FnMut(&mut Vec<u8>));
}
