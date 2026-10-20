use std::error::Error;
use std::vec;

use bevy_ecs::resource::Resource;
use campfire_sim::StableId;
use thiserror::Error;

use crate::scripts::error::{CallError, FailureKind};
use crate::scripts::hook::Hook;

/// The script calls of the running tick that failed. A failed call changed nothing; the list is
/// for the host to see. Not state: it empties each tick. Each failure keeps what its error is and
/// its text, not the error, whose raised value only the match's thread of scripts holds, so the
/// host reads it as a plain resource.
#[derive(Resource, Debug, Default)]
pub struct ScriptFailures(Vec<ScriptFailure>);

/// A call that failed: the unit it ran for, if any, its hook, what its error is, and the error as
/// text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptFailure {
    pub id: Option<StableId>,
    pub hook: Hook,
    pub kind: FailureKind,
    pub error: FailureText,
}

/// A failed call's error as text: its message, then the error it comes from, each of its chain
/// in turn, so a report of it reads as one of the error would.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{message}")]
pub struct FailureText {
    message: String,
    #[source]
    source: Option<Box<Self>>,
}

impl FailureText {
    /// The text of `error` and of each error of its chain.
    fn of(error: &dyn Error) -> Self {
        Self {
            message: error.to_string(),
            source: error.source().map(|source| Box::new(Self::of(source))),
        }
    }
}

impl ScriptFailures {
    pub fn get(&self) -> &[ScriptFailure] {
        &self.0
    }

    /// Takes the failures out, for a caller that keeps them past the tick.
    pub fn drain(&mut self) -> vec::Drain<'_, ScriptFailure> {
        self.0.drain(..)
    }

    pub fn record(&mut self, id: Option<StableId>, hook: Hook, error: &CallError) {
        self.0.push(ScriptFailure {
            id,
            hook,
            kind: error.kind(),
            error: FailureText::of(error),
        });
    }

    /// Forgets the failures of the tick before, as a tick begins.
    pub fn clear(&mut self) {
        self.0.clear();
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_sim::StableId;

    use crate::scripts::error::FailureKind;
    use crate::scripts::hook::Hook;
    use crate::scripts::script_failures::ScriptFailures;

    /// A failed call as a test compares it: its unit, its hook and what its error is.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct FailedCall {
        pub id: Option<StableId>,
        pub hook: Hook,
        pub kind: FailureKind,
    }

    impl ScriptFailures {
        pub fn calls(&self) -> Vec<FailedCall> {
            self.0
                .iter()
                .map(|failure| FailedCall {
                    id: failure.id,
                    hook: failure.hook,
                    kind: failure.kind,
                })
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scripts::error::ApiError;
    use crate::scripts::script_role::ScriptRole;

    #[test]
    fn a_failure_keeps_its_kind_and_each_message_of_its_errors_chain() {
        // A refused call is the API's refusal, whose own message is its source's.
        let error = CallError::Api(ApiError::NegativeRadius);
        let mut failures = ScriptFailures::default();
        let think = Hook {
            name: "on_think",
            role: ScriptRole::Ai,
            param_names: &["ctx", "unit"],
            param_kinds: &[None, None],
        };
        failures.record(None, think, &error);
        let [failure] = failures.get() else {
            panic!("one failure");
        };
        assert_eq!(failure.kind, FailureKind::Api(ApiError::NegativeRadius));
        assert_eq!(failure.error.to_string(), error.to_string());
        let source = failure.error.source().map(ToString::to_string);
        assert_eq!(source, Some(ApiError::NegativeRadius.to_string()));
        assert!(failure.error.source().and_then(Error::source).is_none());
        failures.clear();
        assert!(failures.get().is_empty());
    }
}
