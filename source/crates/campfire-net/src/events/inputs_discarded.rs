use campfire_common::PlayerSlot;
use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::warn;

/// The client took its match up again after its link failed, and the server's copy of the
/// player's chain lacked their last `count` inputs, as a crash lost them or a load dropped them:
/// those never apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct InputsDiscarded {
    pub slot: PlayerSlot,
    pub count: u64,
}

impl LogEvent for InputsDiscarded {
    const MESSAGE: &'static str =
        "the server's log does not hold the player's last inputs; they never apply";

    fn log(&self) {
        warn!(
            slot = self.slot.get(),
            count = self.count,
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
        round_trip(&InputsDiscarded {
            slot: PlayerSlot::new(1),
            count: 3,
        });
    }
}
