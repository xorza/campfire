use campfire_bots::{Named, OrderUnsent};
use campfire_capabilities::Action;
use campfire_common::PlayerSlot;
use campfire_log::{JsonText, LogEvent, LogLine};
use serde::Deserialize;
use tracing::warn;

/// A bot's due order went unsent to all of the units of `slot` it commands or to some, with why
/// and its action as its script names it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct BotOrderUnsent {
    pub slot: PlayerSlot,
    #[serde(deserialize_with = "LogLine::json")]
    pub cause: OrderUnsent,
    #[serde(deserialize_with = "LogLine::json")]
    pub action: Action<Named>,
}

impl LogEvent for BotOrderUnsent {
    const MESSAGE: &'static str =
        "dropped a bot's order: its slot lacks the units, the slot or the target it names";

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
    use campfire_common::DeclaredName;
    use campfire_log::internals::round_trip;
    use campfire_progression::ProgressionAction;

    use super::*;

    #[test]
    fn the_event_reads_back_what_it_logs() {
        let slot = DeclaredName::new("mend").unwrap();
        round_trip(&BotOrderUnsent {
            slot: PlayerSlot::new(1),
            cause: OrderUnsent::NoSlot,
            action: Action::Progression(ProgressionAction::Learn { slot }),
        });
    }
}
