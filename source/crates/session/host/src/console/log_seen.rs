use campfire_console::{FailedScriptCall, ScriptFailureCount};
use campfire_log::LogLine;
use campfire_runner::ScriptCallFailed;
use campfire_sim::StableId;

/// What the log's tap saw that a console reads: the script calls that failed since the console
/// opened, and whether an event of the message a run waits for came since it started to wait.
#[derive(Debug, Default)]
pub struct LogSeen {
    failures: ScriptFailureCount,
    watch: Option<EventWatch>,
}

/// The message a run waits for, and whether an event of it came.
#[derive(Debug)]
struct EventWatch {
    message: String,
    seen: bool,
}

impl LogSeen {
    /// Takes the log's `line`: a failed script call it counts, and an event of the message it
    /// waits for.
    pub fn see(&mut self, line: &LogLine) {
        if let Some(Ok(failed)) = line.read::<ScriptCallFailed>() {
            self.failures.note(FailedScriptCall {
                tick: failed.tick.get(),
                unit: failed.unit.map(StableId::get),
                hook: failed.hook,
                error: failed.error,
            });
        }
        if let Some(watch) = &mut self.watch
            && line.fields.text("message") == Some(watch.message.as_str())
        {
            watch.seen = true;
        }
    }

    pub const fn failures(&self) -> &ScriptFailureCount {
        &self.failures
    }

    /// Waits for an event of `message`, from now on.
    pub fn watch(&mut self, message: String) {
        self.watch = Some(EventWatch {
            message,
            seen: false,
        });
    }

    /// Waits for no event.
    pub fn unwatch(&mut self) {
        self.watch = None;
    }

    /// Whether an event of the message it waits for came.
    pub fn is_watched_seen(&self) -> bool {
        self.watch.as_ref().is_some_and(|watch| watch.seen)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_counts_the_failed_calls_and_sees_the_event_it_waits_for() {
        let line = |text: &str| LogLine::parse(text).unwrap();
        let failed = line(
            r#"{"level":"WARN","target":"campfire_runner","fields":{"message":"a script call failed and changed nothing","tick":7,"unit":4,"hook":"on_hit","error":"no"}}"#,
        );
        let died =
            line(r#"{"level":"INFO","target":"campfire_host","fields":{"message":"a unit died"}}"#);
        let mut seen = LogSeen::default();
        // An event before the watch is not seen; the watch sees the next.
        seen.see(&died);
        seen.watch("a unit died".to_owned());
        assert!(!seen.is_watched_seen());
        seen.see(&failed);
        assert!(!seen.is_watched_seen());
        seen.see(&died);
        assert!(seen.is_watched_seen());
        seen.unwatch();
        assert!(!seen.is_watched_seen());
        assert_eq!(
            seen.failures(),
            &ScriptFailureCount {
                count: 1,
                latest: Some(FailedScriptCall {
                    tick: 7,
                    unit: Some(4),
                    hook: "on_hit".to_owned(),
                    error: "no".to_owned(),
                }),
            }
        );
    }
}
