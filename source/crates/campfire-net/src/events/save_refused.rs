use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::warn;

/// The server refused a player's save command, for `reason`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SaveRefused {
    pub reason: String,
}

impl LogEvent for SaveRefused {
    const MESSAGE: &'static str = "refused a player's save command";

    fn log(&self) {
        warn!(reason = %self.reason, "{}", Self::MESSAGE);
    }
}

#[cfg(test)]
mod tests {
    use campfire_log::internals::round_trip;

    use super::*;

    #[test]
    fn the_event_reads_back_what_it_logs() {
        round_trip(&SaveRefused {
            reason: "the mode alone saves".to_owned(),
        });
    }
}
