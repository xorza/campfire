use campfire_bots::{Named, OrderUnsent};
use campfire_capabilities::Directive;
use campfire_common::Seat;
use campfire_log::{JsonText, LogEvent, LogLine};
use serde::Deserialize;
use tracing::warn;

/// A bot's due order went unsent to all of the units of `seat` it commands or to some, with why
/// and its directive as its script names it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct BotOrderUnsent {
    pub seat: Seat,
    #[serde(deserialize_with = "LogLine::json")]
    pub cause: OrderUnsent,
    #[serde(deserialize_with = "LogLine::json")]
    pub directive: Directive<Named>,
}

impl LogEvent for BotOrderUnsent {
    const MESSAGE: &'static str =
        "dropped a bot's order: its seat lacks the units, the slot or the target it names";

    fn log(&self) {
        warn!(
            seat = self.seat.get(),
            cause = %JsonText(&self.cause),
            directive = %JsonText(&self.directive),
            "{}",
            Self::MESSAGE
        );
    }
}

#[cfg(test)]
mod tests {
    use campfire_common::DeclaredName;
    use campfire_log::internals::round_trip;
    use campfire_progression::ProgressionDirective;

    use super::*;

    #[test]
    fn the_event_reads_back_what_it_logs() {
        let slot = DeclaredName::new("mend").unwrap();
        round_trip(&BotOrderUnsent {
            seat: Seat::new(1),
            cause: OrderUnsent::NoSlot,
            directive: Directive::Progression(ProgressionDirective::Learn { slot }),
        });
    }
}
