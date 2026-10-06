use std::path::PathBuf;

use campfire_common::StateHash;
use campfire_log::{LogEvent, LogLine};
use campfire_protocol::SessionId;
use serde::Deserialize;
use tracing::info;

/// Every player left, and the server wrote the session log to `file`, in its working directory,
/// after a match that ended in the state of `hash`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SessionWritten {
    #[serde(deserialize_with = "LogLine::text")]
    pub session: SessionId,
    pub file: PathBuf,
    #[serde(deserialize_with = "LogLine::text")]
    pub hash: StateHash,
}

impl LogEvent for SessionWritten {
    const MESSAGE: &'static str = "every player left; wrote the session log, which \
                                   `campfire-verifier <packages directory> <file>` replays to \
                                   the same hash";

    fn log(&self) {
        info!(
            session = %self.session,
            file = %self.file.display(),
            hash = %self.hash,
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
        round_trip(&SessionWritten {
            session: SessionId::new([1; 32]),
            file: PathBuf::from("0101.campfire-log"),
            hash: "ab".repeat(32).parse().unwrap(),
        });
    }
}
