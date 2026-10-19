use campfire_log::{LogEvent, LogLine};
use campfire_protocol::SessionId;
use serde::Deserialize;
use tracing::warn;

/// A restore found a frame of the session's journal torn or corrupt, and cut the journal there:
/// the `bytes` from it to the end are gone, and the session goes on from the last whole frame
/// (08-sessions.md, A journal is cut at its first bad frame).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct JournalCut {
    #[serde(deserialize_with = "LogLine::text")]
    pub session: SessionId,
    pub bytes: u64,
}

impl JournalCut {
    /// Logs the cut when it drops a byte.
    pub(crate) fn log_dropped(&self) {
        if self.bytes > 0 {
            self.log();
        }
    }
}

impl LogEvent for JournalCut {
    const MESSAGE: &'static str = "cut the journal at its first bad frame";

    fn log(&self) {
        warn!(session = %self.session, bytes = self.bytes, "{}", Self::MESSAGE);
    }
}

#[cfg(test)]
mod tests {
    use campfire_log::internals::round_trip;

    use super::*;

    #[test]
    fn the_event_reads_back_what_it_logs() {
        round_trip(&JournalCut {
            session: SessionId::new([1; 32]),
            bytes: 17,
        });
    }
}
