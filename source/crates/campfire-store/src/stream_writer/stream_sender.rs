use std::sync::Arc;

use crate::stream_writer::StreamShared;

/// Puts pieces into a `StreamWriter`'s buffer, waiting while it is full; see `StreamWriter`.
#[derive(Debug, Clone)]
pub struct StreamSender(pub(super) Arc<StreamShared>);

impl StreamSender {
    /// Puts the piece `put` writes at the end of the buffer, for the worker; nothing once the
    /// worker stopped at a failure, or the writer dropped.
    pub fn put(&self, put: impl FnOnce(&mut Vec<u8>)) {
        self.0.put(put);
    }
}
