use std::path::PathBuf;

use campfire_common::StateHash;
use campfire_log::{LogEvent, LogLine};
use serde::Deserialize;
use tracing::info;

/// The log `file` verifies, and its replay ends in the state of `hash`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Verified {
    pub file: PathBuf,
    #[serde(deserialize_with = "LogLine::text")]
    pub hash: StateHash,
}

impl LogEvent for Verified {
    const MESSAGE: &'static str = "the log verifies; its final state hash";

    fn log(&self) {
        info!(file = %self.file.display(), hash = %self.hash, "{}", Self::MESSAGE);
    }
}

#[cfg(test)]
mod tests {
    use campfire_log::internals::round_trip;

    use super::*;

    #[test]
    fn the_event_reads_back_what_it_logs() {
        round_trip(&Verified {
            file: PathBuf::from("0101.campfire-log"),
            hash: "cd".repeat(32).parse().unwrap(),
        });
    }
}
