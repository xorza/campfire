use std::error::Error;
use std::fmt::{self, Write};

use bevy_ecs::resource::Resource;
use campfire_sim::StableId;

use crate::scripts::error::{CallError, FailureKind};
use crate::scripts::hook::Hook;

/// The script calls of the running tick that failed. A failed call changed nothing; the list is
/// for the host to see. Not state: it empties each tick. Each failure keeps what its error is and
/// its text, not the error, whose raised value only the match's thread of scripts holds, so the
/// host reads it as a plain resource; the texts of every failure share one buffer, so a failure
/// allocates nothing once the buffers have grown.
#[derive(Resource, Default)]
pub struct ScriptFailures {
    failures: Vec<ScriptFailure>,
    /// The messages of every failure's error chain, one after another.
    text: String,
    /// Where each message ends in `text`.
    ends: Vec<u32>,
}

/// A call that failed: the unit it ran for, if any, its hook, what its error is, and where its
/// text lies among its failures'.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptFailure {
    pub id: Option<StableId>,
    pub hook: Hook,
    pub kind: FailureKind,
    /// Its messages among the failures' ends, the first and the one past the last.
    messages: [u32; 2],
}

/// A failed call's error as text: its message, then the message of each error of its chain in
/// turn. It writes as a report of the error does, each joined by `: `.
#[derive(Debug, Clone, Copy)]
pub struct FailureText<'a> {
    text: &'a str,
    /// Where each of its messages ends in `text`, and where the first starts.
    ends: &'a [u32],
    start: u32,
}

impl FailureText<'_> {
    /// The messages of its chain, its own first.
    fn messages(&self) -> impl Iterator<Item = &str> + '_ {
        let starts = std::iter::once(self.start).chain(self.ends.iter().copied());
        starts
            .zip(self.ends)
            .map(|(start, &end)| &self.text[start as usize..end as usize])
    }

    /// The error's own message, with none of its chain.
    pub fn message(&self) -> &str {
        self.messages().next().unwrap_or_default()
    }
}

impl fmt::Display for FailureText<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (at, message) in self.messages().enumerate() {
            if at > 0 {
                f.write_str(": ")?;
            }
            f.write_str(message)?;
        }
        Ok(())
    }
}

impl ScriptFailures {
    pub fn get(&self) -> &[ScriptFailure] {
        &self.failures
    }

    pub const fn is_empty(&self) -> bool {
        self.failures.is_empty()
    }

    /// The text of `failure`, one of its failures.
    pub fn text(&self, failure: &ScriptFailure) -> FailureText<'_> {
        let [first, end] = failure.messages.map(|at| at as usize);
        let start = first.checked_sub(1).map_or(0, |before| self.ends[before]);
        FailureText {
            text: &self.text,
            ends: &self.ends[first..end],
            start,
        }
    }

    /// Moves the failures into `kept`, after its own, for a caller that keeps them past the
    /// tick.
    pub fn move_into(&mut self, kept: &mut Self) {
        let ends = u32::try_from(kept.ends.len()).expect("the messages fit u32");
        let text = u32::try_from(kept.text.len()).expect("the failures' text fits u32");
        kept.failures
            .extend(self.failures.drain(..).map(|failure| ScriptFailure {
                messages: failure.messages.map(|at| at + ends),
                ..failure
            }));
        kept.ends.extend(self.ends.drain(..).map(|end| end + text));
        kept.text.push_str(&self.text);
        self.text.clear();
    }

    pub fn record(&mut self, id: Option<StableId>, hook: Hook, error: &CallError) {
        let first = self.ends.len();
        let mut link: Option<&dyn Error> = Some(error);
        while let Some(error) = link {
            write!(self.text, "{error}").expect("text writes into a string");
            let end = u32::try_from(self.text.len()).expect("the failures' text fits u32");
            self.ends.push(end);
            link = error.source();
        }
        let at = |at: usize| u32::try_from(at).expect("the messages fit u32");
        self.failures.push(ScriptFailure {
            id,
            hook,
            kind: error.kind(),
            messages: [at(first), at(self.ends.len())],
        });
    }

    /// Forgets the failures of the tick before, as a tick begins. A build of `internals` fails
    /// on any no host took, whatever drives the match, so no test passes with a failed call it
    /// does not expect (02-engine-core.md, A test fails on a script failure it does not take).
    pub fn clear(&mut self) {
        #[cfg(feature = "internals")]
        assert!(
            self.failures.is_empty(),
            "script calls failed in the last tick: {self:?}; a test that makes them takes them"
        );
        self.forget();
    }

    /// Forgets the failures a host took, as one that logged them does, keeping the buffers.
    pub fn forget(&mut self) {
        self.failures.clear();
        self.text.clear();
        self.ends.clear();
    }
}

/// Each failure, with its text.
impl fmt::Debug for ScriptFailures {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut list = f.debug_list();
        for failure in &self.failures {
            list.entry(&(
                failure.id,
                failure.hook.name,
                failure.kind,
                self.text(failure).to_string(),
            ));
        }
        list.finish()
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

    /// A match dropped after a tick whose failed calls nothing took fails its test, as its next
    /// tick's start would (02-engine-core.md, A test fails on a script failure it does not take).
    impl Drop for ScriptFailures {
        fn drop(&mut self) {
            assert!(
                self.failures.is_empty() || std::thread::panicking(),
                "script calls failed in the last tick: {self:?}; a test that makes them takes them"
            );
        }
    }

    impl ScriptFailures {
        /// Takes the failed calls of the last tick, as a test that expects them does.
        pub fn take_calls(&mut self) -> Vec<FailedCall> {
            let calls = self
                .failures
                .iter()
                .map(|failure| FailedCall {
                    id: failure.id,
                    hook: failure.hook,
                    kind: failure.kind,
                })
                .collect();
            self.failures.clear();
            self.text.clear();
            self.ends.clear();
            calls
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scripts::error::ApiError;
    use crate::scripts::script_role::ScriptRole;

    /// The hook of the failures the tests record.
    const THINK: Hook = Hook {
        name: "on_think",
        role: ScriptRole::Ai,
        param_names: &["ctx", "unit"],
        param_kinds: &[None, None],
    };

    #[test]
    fn a_failure_keeps_its_kind_and_each_message_of_its_errors_chain() {
        // A refused call is the API's refusal, whose own message is its source's.
        let error = CallError::Api(ApiError::NegativeRadius);
        let mut failures = ScriptFailures::default();
        failures.record(None, THINK, &error);
        let [failure] = failures.get() else {
            panic!("one failure");
        };
        assert_eq!(failure.kind, FailureKind::Api(ApiError::NegativeRadius));
        let text = failures.text(failure);
        assert_eq!(text.message(), error.to_string());
        let report = format!("{error}: {}", ApiError::NegativeRadius);
        assert_eq!(text.to_string(), report);
        // A second failure's text follows the first's; moved into a list it keeps it.
        failures.record(None, THINK, &CallError::Api(ApiError::PureCall));
        let mut kept = ScriptFailures::default();
        kept.record(None, THINK, &error);
        failures.move_into(&mut kept);
        assert!(failures.is_empty());
        let texts: Vec<String> = kept
            .get()
            .iter()
            .map(|failure| kept.text(failure).to_string())
            .collect();
        let pure = CallError::Api(ApiError::PureCall);
        let pure = format!("{pure}: {}", ApiError::PureCall);
        assert_eq!(texts, [report.clone(), report, pure]);
        kept.forget();
        failures.clear();
        assert!(failures.get().is_empty());
        // A host that logged them forgets them, and the next tick begins clean.
        failures.record(None, THINK, &error);
        failures.forget();
        failures.clear();
    }

    #[test]
    #[should_panic(expected = "script calls failed in the last tick")]
    fn a_tick_that_begins_over_a_failure_no_host_took_fails_the_test() {
        let mut failures = ScriptFailures::default();
        failures.record(None, THINK, &CallError::Api(ApiError::PureCall));
        failures.clear();
    }

    #[test]
    #[should_panic(expected = "script calls failed in the last tick")]
    fn a_match_dropped_over_a_failure_no_host_took_fails_the_test() {
        let mut failures = ScriptFailures::default();
        failures.record(None, THINK, &CallError::Api(ApiError::PureCall));
        drop(failures);
    }
}
