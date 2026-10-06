use campfire_common::PlayerSlot;
use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::warn;

/// A server bot's order for the avatar of `slot` found none, and was dropped; `action` is the
/// order's action, as its `Debug` writes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AvatarMissing {
    pub slot: PlayerSlot,
    pub action: String,
}

impl LogEvent for AvatarMissing {
    const MESSAGE: &'static str = "dropped a bot's order: its slot has no avatar";

    fn log(&self) {
        warn!(slot = self.slot.get(), action = %self.action, "{}", Self::MESSAGE);
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
            action: "Learn { slot: 0 }".to_owned(),
        });
    }
}
