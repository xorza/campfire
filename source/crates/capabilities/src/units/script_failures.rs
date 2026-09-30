use campfire_sim::StableId;

use crate::units::error::CallError;
use crate::units::hook::Hook;

/// The script calls of the running tick that failed. A failed call changed nothing; the list is
/// for the host to see. Not state: it empties each tick. A non-send resource, as a script's
/// raised value is not `Send`.
#[derive(Debug, Default)]
pub struct ScriptFailures(pub(crate) Vec<ScriptFailure>);

/// A call that failed: the unit it ran for, if any, its hook, and why.
#[derive(Debug, Clone)]
pub struct ScriptFailure {
    pub unit: Option<StableId>,
    pub hook: Hook,
    pub error: CallError,
}

impl ScriptFailures {
    pub fn get(&self) -> &[ScriptFailure] {
        &self.0
    }

    pub(crate) fn record(&mut self, unit: Option<StableId>, hook: Hook, error: CallError) {
        self.0.push(ScriptFailure { unit, hook, error });
    }
}
