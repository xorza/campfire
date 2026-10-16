use campfire_log::{JsonText, LogEvent, LogLine};
use serde::Deserialize;
use tracing::warn;

use crate::error::SaveRefusal;

/// The server refused a player's save command, for `refusal`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct SaveRefused {
    #[serde(deserialize_with = "LogLine::json")]
    pub refusal: SaveRefusal,
}

impl LogEvent for SaveRefused {
    const MESSAGE: &'static str = "refused a player's save command";

    fn log(&self) {
        warn!(refusal = %JsonText(&self.refusal), "{}", Self::MESSAGE);
    }
}

#[cfg(test)]
mod tests {
    use campfire_log::internals::round_trip;

    use super::*;

    #[test]
    fn the_event_reads_back_what_it_logs() {
        for refusal in [
            SaveRefusal::NotLocal,
            SaveRefusal::ByMode,
            SaveRefusal::NoData,
            SaveRefusal::NoSave,
        ] {
            round_trip(&SaveRefused { refusal });
        }
    }
}
