use campfire_log::{LogEvent, LogLine};
use campfire_protocol::SessionId;
use serde::Deserialize;
use tracing::warn;

/// The client refused the offered `session`, whose terms do not fit, which ends the link;
/// `mismatch` says why.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SessionRefused {
    #[serde(deserialize_with = "LogLine::text")]
    pub session: SessionId,
    pub mismatch: String,
}

impl LogEvent for SessionRefused {
    const MESSAGE: &'static str = "refused the offered session, which ends the link";

    fn log(&self) {
        warn!(
            session = %self.session,
            mismatch = %self.mismatch,
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
        round_trip(&SessionRefused {
            session: SessionId::new([7; 32]),
            mismatch: "the server's key is not the one given".to_owned(),
        });
    }
}
