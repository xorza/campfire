use campfire_capabilities::Action;
use campfire_common::PlayerSlot;
use campfire_log::{JsonText, LogEvent, LogLine};
use serde::{Deserialize, Serialize};
use tracing::warn;

/// A bot's order found none of the units of `slot` it names, and was dropped, with why and its
/// action.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BotUnitsMissing {
    pub slot: PlayerSlot,
    #[serde(deserialize_with = "LogLine::json")]
    pub cause: UnitsMissing,
    #[serde(deserialize_with = "LogLine::json")]
    pub action: Action,
}

/// Why a bot's order commands no unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitsMissing {
    /// It is for the avatar, and the slot has none.
    NoAvatar,
    /// It names a unit type the match does not hold.
    UnknownType,
    /// The slot owns no living unit of the type it names.
    NoUnit,
}

impl LogEvent for BotUnitsMissing {
    const MESSAGE: &'static str = "dropped a bot's order: its slot has none of the units it names";

    fn log(&self) {
        warn!(
            slot = self.slot.get(),
            cause = %JsonText(&self.cause),
            action = %JsonText(&self.action),
            "{}",
            Self::MESSAGE
        );
    }
}

#[cfg(test)]
mod tests {
    use campfire_log::internals::round_trip;
    use campfire_progression::ProgressionAction;

    use super::*;

    #[test]
    fn the_event_reads_back_what_it_logs() {
        round_trip(&BotUnitsMissing {
            slot: PlayerSlot::new(1),
            cause: UnitsMissing::NoUnit,
            action: Action::Progression(ProgressionAction::Learn { slot: 0 }),
        });
    }
}
