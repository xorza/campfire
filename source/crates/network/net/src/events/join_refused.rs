use campfire_log::{JsonText, LogEvent, LogLine};
use serde::Deserialize;
use tracing::warn;

use crate::error::JoinError;

/// The server refused the join of `link`, its entity's bits, as the lobby's other lines name it,
/// for `error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct JoinRefused {
    pub link: u64,
    #[serde(deserialize_with = "LogLine::json")]
    pub error: JoinError,
}

impl LogEvent for JoinRefused {
    const MESSAGE: &'static str = "refused a join";

    fn log(&self) {
        warn!(link = self.link, error = %JsonText(&self.error), "{}", Self::MESSAGE);
    }
}

#[cfg(test)]
mod tests {
    use campfire_log::internals::round_trip;
    use campfire_protocol::{DelegationError, EventError};

    use super::*;

    #[test]
    fn the_event_reads_back_what_it_logs() {
        for error in [
            JoinError::Full,
            JoinError::Delegation(DelegationError::NotEvent(EventError::Malformed)),
        ] {
            round_trip(&JoinRefused { link: 14, error });
        }
    }
}
