use campfire_common::PlayerSlot;
use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::warn;

/// The server refused an input message of `slot` whose frames do not fit its payloads, which
/// ends the link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct InputMessageUnfit {
    pub slot: PlayerSlot,
}

impl LogEvent for InputMessageUnfit {
    const MESSAGE: &'static str = "refused an input message whose frames do not fit its payloads";

    fn log(&self) {
        warn!(slot = self.slot.get(), "{}", Self::MESSAGE);
    }
}

#[cfg(test)]
mod tests {
    use campfire_log::internals::round_trip;

    use super::*;

    #[test]
    fn the_event_reads_back_what_it_logs() {
        round_trip(&InputMessageUnfit {
            slot: PlayerSlot::new(1),
        });
    }
}
