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
mod tests;
