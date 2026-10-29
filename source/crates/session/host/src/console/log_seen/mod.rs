use campfire_console::{FailedScriptCall, ScriptFailureCount};
use campfire_log::LogRecord;
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
    /// Takes the log's `record`: a failed script call it counts, and an event of the message it
    /// waits for.
    pub fn see(&mut self, record: &LogRecord) {
        if let Some(Ok(failed)) = record.read::<ScriptCallFailed>() {
            self.failures.note(FailedScriptCall {
                tick: failed.tick.get(),
                unit: failed.unit.map(StableId::get),
                hook: failed.hook,
                error: failed.error,
            });
        }
        if let Some(watch) = &mut self.watch
            && record.text("message") == Some(watch.message.as_str())
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
mod tests;
