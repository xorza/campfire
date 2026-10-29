use std::fmt;
use std::sync::{Arc, OnceLock};

use campfire_log::{LogRecord, RecordLayer};
use tracing::Subscriber;
use tracing_subscriber::Layer;

/// What a tap gives each record to.
type TakeRecord = Box<dyn Fn(&LogRecord) + Send + Sync>;

/// The log's events as a console takes them, once it starts the tap: each event's record, its
/// fields typed as tracing records them, by the log file's filter, given to the console's taker
/// on the thread that logs it, with no line written and parsed back. None before.
#[derive(Clone, Default)]
pub struct LogTap(Arc<OnceLock<TakeRecord>>);

impl LogTap {
    /// Gives each record logged from now on to `take`, which logs nothing itself: a tap starts
    /// once.
    pub fn start(&self, take: impl Fn(&LogRecord) + Send + Sync + 'static) {
        let started = self.0.set(Box::new(take));
        assert!(started.is_ok(), "a log's tap starts once");
    }

    /// Whether a console took the tap.
    pub(crate) fn is_started(&self) -> bool {
        self.0.get().is_some()
    }

    /// The tap's layer, which hands each event's record to the tap's taker once one took it.
    pub(crate) fn layer<S: Subscriber>(&self) -> impl Layer<S> + use<S> {
        let tap = self.clone();
        RecordLayer::new(move |record: &LogRecord| {
            if let Some(take) = tap.0.get() {
                take(record);
            }
        })
    }
}

impl fmt::Debug for LogTap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("LogTap").field(&self.is_started()).finish()
    }
}

#[cfg(test)]
mod tests;
