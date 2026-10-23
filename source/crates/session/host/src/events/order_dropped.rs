use campfire_capabilities::Directive;
use campfire_log::{JsonText, LogEvent, LogLine};
use campfire_sim::StableId;
use serde::Deserialize;
use tracing::warn;

/// The client dropped an order to `units` units, the first of them `unit`, whose payload passes
/// the session's max length, with its directive.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct OrderDropped {
    pub unit: StableId,
    pub units: usize,
    #[serde(deserialize_with = "LogLine::json")]
    pub directive: Directive,
}

impl LogEvent for OrderDropped {
    const MESSAGE: &'static str = "dropped an order whose payload passes the session's max length";

    fn log(&self) {
        warn!(
            unit = self.unit.get(),
            units = self.units,
            directive = %JsonText(&self.directive),
            "{}",
            Self::MESSAGE
        );
    }
}

#[cfg(test)]
mod tests {
    use campfire_log::internals::round_trip;
    use campfire_progression::ProgressionDirective;
    use campfire_sim::IdAllocator;

    use super::*;

    #[test]
    fn the_event_reads_back_what_it_logs() {
        round_trip(&OrderDropped {
            unit: IdAllocator::default().allocate(),
            units: 3,
            directive: Directive::Progression(ProgressionDirective::Learn { slot: 2 }),
        });
    }
}
