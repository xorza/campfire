use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::warn;

/// A bot dropped its mode input `name`, whose payload passes the session's max length.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct InputDropped {
    pub name: String,
}

impl LogEvent for InputDropped {
    const MESSAGE: &'static str =
        "dropped a mode input whose payload passes the session's max length";

    fn log(&self) {
        warn!(name = %self.name, "{}", Self::MESSAGE);
    }
}

#[cfg(test)]
mod tests {
    use campfire_log::internals::round_trip;

    use super::*;

    #[test]
    fn the_event_reads_back_what_it_logs() {
        round_trip(&InputDropped {
            name: "hero".to_owned(),
        });
    }
}
