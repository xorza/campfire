use std::path::PathBuf;

use campfire_log::{LogEvent, LogLine};
use campfire_protocol::SessionId;
use serde::Deserialize;
use tracing::info;

/// The server started again past the restore window of the session a stop ended, or after it
/// ended: it logged the session's end and published its log to `file`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SessionAborted {
    #[serde(deserialize_with = "LogLine::text")]
    pub session: SessionId,
    pub file: PathBuf,
}

impl LogEvent for SessionAborted {
    const MESSAGE: &'static str = "ended the session a stop left, and published its log";

    fn log(&self) {
        info!(
            session = %self.session,
            file = %self.file.display(),
            "{}",
            Self::MESSAGE
        );
    }
}

#[cfg(test)]
mod tests {
    use campfire_log::internals::round_trip;

    use super::*;

    #[test]
    fn the_event_reads_back_what_it_logs() {
        round_trip(&SessionAborted {
            session: SessionId::new([1; 32]),
            file: PathBuf::from("logs/0101.campfire-log"),
        });
    }
}
