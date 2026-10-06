use campfire_capabilities::Hook;
use campfire_common::Tick;
use campfire_log::LogEvent;
use campfire_sim::StableId;
use serde::Deserialize;
use tracing::warn;

/// A script call of `hook` failed in `tick` and changed nothing: for `unit`, when it ran for one;
/// `error` says why.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ScriptCallFailed {
    pub tick: Tick,
    pub unit: Option<StableId>,
    pub hook: Hook,
    pub error: String,
}

impl LogEvent for ScriptCallFailed {
    const MESSAGE: &'static str = "a script call failed and changed nothing";

    fn log(&self) {
        warn!(
            tick = self.tick.get(),
            unit = self.unit.map(StableId::get),
            hook = ?self.hook,
            error = %self.error,
            "{}",
            Self::MESSAGE
        );
    }
}

#[cfg(test)]
mod tests {
    use campfire_log::internals::round_trip;
    use campfire_sim::IdAllocator;

    use super::*;

    #[test]
    fn the_event_reads_back_what_it_logs() {
        let failed = ScriptCallFailed {
            tick: Tick::new(9),
            unit: Some(IdAllocator::default().allocate()),
            hook: Hook::OnHit,
            error: "the unit has no param `power`".to_owned(),
        };
        round_trip(&failed);
        // A call for no unit leaves the field out.
        round_trip(&ScriptCallFailed {
            unit: None,
            hook: Hook::OnMatchStart,
            ..failed
        });
    }
}
