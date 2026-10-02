use campfire_sim::StableId;

use crate::scripts::error::CallError;
use crate::scripts::hook::Hook;

/// The script calls of the running tick that failed. A failed call changed nothing; the list is
/// for the host to see. Not state: it empties each tick. A non-send resource, as a script's
/// raised value is not `Send`.
#[derive(Debug, Default)]
pub struct ScriptFailures(Vec<ScriptFailure>);

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

    /// Forgets the failures of the tick before, as a tick begins.
    pub(crate) fn clear(&mut self) {
        self.0.clear();
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use campfire_sim::StableId;

    use crate::scripts::error::internals::FailureKind;
    use crate::scripts::hook::Hook;
    use crate::scripts::script_failures::ScriptFailures;

    /// A failed call as a test compares it: its unit, its hook and what its error is.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) struct FailedCall {
        pub(crate) unit: Option<StableId>,
        pub(crate) hook: Hook,
        pub(crate) kind: FailureKind,
    }

    impl ScriptFailures {
        pub(crate) fn calls(&self) -> Vec<FailedCall> {
            self.0
                .iter()
                .map(|failure| FailedCall {
                    unit: failure.unit,
                    hook: failure.hook,
                    kind: failure.error.kind(),
                })
                .collect()
        }
    }
}
