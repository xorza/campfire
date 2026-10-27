use std::fmt;
use std::io::{self, Write};
use std::str;
use std::sync::{Arc, OnceLock};

use tracing_subscriber::fmt::MakeWriter;

/// What a tap gives each line to.
type TakeLine = Box<dyn Fn(&str) + Send + Sync>;

/// The log's events as a console takes them, once it starts the tap: each a JSON line as the log
/// file holds it, by the log file's filter, given whole to the console's taker on the thread that
/// logs it. None before, when the tap formats no line.
#[derive(Clone, Default)]
pub struct LogTap(Arc<OnceLock<TakeLine>>);

/// The tap layer's writer: each event's line, which the layer writes whole.
#[derive(Debug, Clone)]
pub(crate) struct TapLines(pub(crate) LogTap);

/// One event's line, as the layer writes it.
#[derive(Debug)]
pub(crate) struct TapWriter<'a>(&'a LogTap);

impl LogTap {
    /// Gives each line logged from now on to `take`, which logs nothing itself: a tap starts once.
    pub fn start(&self, take: impl Fn(&str) + Send + Sync + 'static) {
        let started = self.0.set(Box::new(take));
        assert!(started.is_ok(), "a log's tap starts once");
    }

    /// Whether a console took the tap.
    pub(crate) fn is_started(&self) -> bool {
        self.0.get().is_some()
    }
}

impl fmt::Debug for LogTap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("LogTap").field(&self.is_started()).finish()
    }
}

impl<'a> MakeWriter<'a> for TapLines {
    type Writer = TapWriter<'a>;

    fn make_writer(&'a self) -> TapWriter<'a> {
        TapWriter(&self.0)
    }
}

impl Write for TapWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if let Some(take) = self.0.0.get() {
            let line = str::from_utf8(bytes).expect("the JSON layer writes UTF-8");
            take(line.trim_end_matches('\n'));
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::panic::{self, AssertUnwindSafe};
    use std::sync::Mutex;

    use campfire_log::{LogLine, json_lines};
    use tracing::subscriber;
    use tracing_subscriber::layer::SubscriberExt;

    use super::*;

    #[test]
    fn a_tap_gives_each_line_whole_once_it_starts() {
        let tap = LogTap::default();
        let lines = Arc::new(Mutex::new(Vec::<String>::new()));
        let layer = json_lines(TapLines(tap.clone()));
        subscriber::with_default(tracing_subscriber::registry().with(layer), || {
            tracing::info!(n = 1, "before");
            let taken = Arc::clone(&lines);
            tap.start(move |line| taken.lock().unwrap().push(line.to_owned()));
            tracing::info!(n = 2, "after");
        });
        let lines = lines.lock().unwrap();
        let [line] = lines.as_slice() else {
            panic!("{lines:?}");
        };
        // One line, with no line break of its own.
        assert!(!line.contains('\n'), "{line}");
        let read = LogLine::parse(line).unwrap();
        assert_eq!(read.fields.text("message"), Some("after"));
        assert!(tap.is_started());
        let again = panic::catch_unwind(AssertUnwindSafe(|| tap.start(|_| {})));
        assert!(again.is_err());
    }
}
