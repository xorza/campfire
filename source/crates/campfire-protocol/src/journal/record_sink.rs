use std::fmt::Debug;

/// Where a session log's journal records go as the log takes them: a file's writer on a server,
/// a buffer under test. The log does no IO itself.
pub trait RecordSink: Debug {
    /// Takes one record, which `write` frames at the end of the buffer it is given.
    fn append(&self, write: &mut dyn FnMut(&mut Vec<u8>));
}
