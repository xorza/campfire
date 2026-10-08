use campfire_capabilities::Action;
use campfire_common::PlayerSlot;
use campfire_log::{JsonText, LogEvent, LogLine};
use serde::Deserialize;
use tracing::warn;

/// A server bot's order for the avatar of `slot` found none, and was dropped, with its action.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AvatarMissing {
    pub slot: PlayerSlot,
    #[serde(deserialize_with = "LogLine::json")]
    pub action: Action,
}

impl LogEvent for AvatarMissing {
    const MESSAGE: &'static str = "dropped a bot's order: its slot has no avatar";

    fn log(&self) {
        warn!(
            slot = self.slot.get(),
            action = %JsonText(&self.action),
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
        round_trip(&AvatarMissing {
            slot: PlayerSlot::new(1),
            action: Action::Learn { slot: 0 },
        });
    }
}
