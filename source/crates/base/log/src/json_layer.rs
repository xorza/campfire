use tracing::Subscriber;
use tracing_subscriber::fmt;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::fmt::format::{Format, Json, JsonFields};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{Layer, Registry};

/// The layer of a binary's JSON lines and of a test's capture of them: an event a line, as JSON,
/// with its spans.
pub fn json_lines<S, W>(writer: W) -> fmt::Layer<S, JsonFields, Format<Json>, W>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    W: for<'w> MakeWriter<'w> + 'static,
{
    fmt::layer()
        .json()
        .with_current_span(true)
        .with_span_list(true)
        .with_writer(writer)
}

/// The layer that keeps the events a `LogCheck` reads, for a binary's log: in a build that holds
/// the check, as a check that plays a test fixture's match, the capture; none in the build that
/// ships. So the feature that makes the check makes its capture, and the process's one subscriber
/// is the binary's log.
pub fn check_layer() -> Option<Box<dyn Layer<Registry> + Send + Sync>> {
    #[cfg(feature = "internals")]
    let layer = Some(internals::capture_layer());
    #[cfg(not(feature = "internals"))]
    let layer = None;
    layer
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::env;
    use std::fmt::Debug;
    use std::io::{self, Write};
    use std::mem;
    use std::sync::{Arc, Mutex, OnceLock, PoisonError, Weak};
    use std::thread;

    use tracing::subscriber;
    use tracing::{Level, Subscriber};
    use tracing_subscriber::filter::{self, FilterFn, Targets};
    use tracing_subscriber::fmt;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::registry::LookupSpan;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::{Layer, Registry};

    use crate::json_layer::json_lines;
    use crate::log_event::LogEvent;
    use crate::log_line::LogLine;
    use crate::log_record::{LogRecord, RecordLayer};
    use campfire_common::ErrorReport;

    /// Asserts that `event` logs one JSON line, which reads back as `event`.
    pub fn round_trip<E: LogEvent + PartialEq + Debug>(event: &E) {
        let lines = capture(|| event.log());
        assert_eq!(lines.len(), 1, "{lines:?}");
        let line = LogLine::parse(&lines[0]).unwrap();
        let read = line.read::<E>().expect("the line is the event");
        assert_eq!(read.as_ref().ok(), Some(event), "{}: {read:?}", lines[0]);
        // Its record, as a console's tap takes it, reads back as the event too.
        let records = Arc::new(Mutex::new(Vec::new()));
        let taken = Arc::clone(&records);
        let layer = RecordLayer::new(move |record: &LogRecord| {
            taken.lock().unwrap().push(record.clone());
        });
        subscriber::with_default(tracing_subscriber::registry().with(layer), || event.log());
        let records = records.lock().unwrap();
        let [record] = records.as_slice() else {
            panic!("{records:?}");
        };
        let read = record.read::<E>().expect("the record is the event");
        assert_eq!(read.as_ref().ok(), Some(event), "{record:?}: {read:?}");
    }

    /// The lines the process's capture keeps, once a check or a binary's log put it in: statics,
    /// as tracing's global subscriber takes no state of the test that holds it.
    static LINES: OnceLock<Arc<Mutex<Vec<u8>>>> = OnceLock::new();

    /// The check the process's fixtures share, while one of them holds it.
    static SHARED: Mutex<Weak<Kept>> = Mutex::new(Weak::new());

    /// The events at Warn and Error that the code a test runs logs on any thread of its process,
    /// records of the `log` crate among them, as nextest runs each test in a process of its own.
    /// A test takes those it causes; when the process's last check drops, each event left fails
    /// the test, as the LAN check fails a match that logs one. In a binary that logs, as a check
    /// that plays a fixture's match, the capture is a layer of the binary's own log.
    #[derive(Debug)]
    pub struct LogCheck(Arc<Kept>);

    /// What a closure gave, and the check of the events it logged.
    #[derive(Debug)]
    pub struct Scoped<R> {
        pub value: R,
        pub check: LogCheck,
    }

    /// The events a check kept, as JSON lines.
    #[derive(Debug)]
    struct Kept {
        lines: Arc<Mutex<Vec<u8>>>,
    }

    impl LogCheck {
        /// The process's check: the one its fixtures share, or a new one, which finds no event
        /// before it that no check took; the first makes the process's subscriber, unless a
        /// binary's log holds the capture already.
        pub fn start() -> Self {
            let mut shared = SHARED.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(held) = shared.upgrade() {
                return Self(held);
            }
            if LINES.get().is_none() {
                tracing_subscriber::registry()
                    .with(capture_layer())
                    .with(shown_layer())
                    .try_init()
                    .expect("a process's subscriber is a check's alone");
            }
            let lines = Arc::clone(LINES.get().expect("the capture keeps its lines"));
            {
                let held = lines.lock().unwrap();
                assert!(
                    held.is_empty(),
                    "the test logged events at Warn or Error with no check alive:\n{}",
                    String::from_utf8_lossy(&held)
                );
            }
            let kept = Arc::new(Kept { lines });
            *shared = Arc::downgrade(&kept);
            Self(kept)
        }

        /// Runs `body` with the events at Warn and Error it logs on this thread kept by a check of
        /// its own, apart from the process's, so a body that runs beside others in one process,
        /// as a seed of a swarm, takes and answers for its own.
        pub fn scoped<R>(body: impl FnOnce() -> R) -> Scoped<R> {
            let lines = Arc::new(Mutex::new(Vec::new()));
            let writer = {
                let lines = Arc::clone(&lines);
                move || Captured(Arc::clone(&lines))
            };
            let layer = json_lines(writer).without_time().with_filter(warnings());
            let value = subscriber::with_default(tracing_subscriber::registry().with(layer), body);
            Scoped {
                value,
                check: Self(Arc::new(Kept { lines })),
            }
        }

        /// Removes the events no `take` read, as their lines, for a test that reports them itself
        /// rather than fail as the check drops.
        pub fn untaken(&self) -> Vec<String> {
            let lines = mem::take(&mut *self.0.lines.lock().unwrap());
            String::from_utf8(lines)
                .unwrap()
                .lines()
                .map(str::to_owned)
                .collect()
        }

        /// Removes the kept events of `E`, and reads them, in the order they came.
        pub fn take<E: LogEvent>(&self) -> Vec<E> {
            let mut lines = self.0.lines.lock().unwrap();
            let text = String::from_utf8(mem::take(&mut *lines)).unwrap();
            let mut taken = Vec::new();
            for line in text.lines() {
                if let Some(event) = LogLine::parse(line).unwrap().read::<E>() {
                    taken.push(
                        event.unwrap_or_else(|error| panic!("{line}: {}", ErrorReport::of(&error))),
                    );
                } else {
                    lines.extend_from_slice(line.as_bytes());
                    lines.push(b'\n');
                }
            }
            taken
        }
    }

    /// The layer of the process's capture, which keeps its events at Warn and Error, with no
    /// time, so a failure names the same lines in every run: the process's subscriber of a test,
    /// or a layer of a binary's log. One in a process.
    pub fn capture_layer() -> Box<dyn Layer<Registry> + Send + Sync> {
        let lines = Arc::new(Mutex::new(Vec::new()));
        assert!(
            LINES.set(Arc::clone(&lines)).is_ok(),
            "a process holds one capture"
        );
        let writer = move || Captured(Arc::clone(&lines));
        Box::new(json_lines(writer).without_time().with_filter(warnings()))
    }

    /// The events `RUST_LOG` names, by its targets' syntax, on standard error, which nextest shows
    /// for a test that fails, so a failure's events below Warn read with no change of code; none
    /// when it is not set, so a test pays nothing for it.
    fn shown_layer<S: Subscriber + for<'a> LookupSpan<'a>>() -> Option<impl Layer<S>> {
        let named = env::var("RUST_LOG").ok()?;
        let targets: Targets = named
            .parse()
            .unwrap_or_else(|error| panic!("RUST_LOG={named:?} names no targets: {error}"));
        Some(fmt::layer().with_writer(io::stderr).with_filter(targets))
    }

    /// The events at Warn and Error, and no span: a check reads an event's own fields alone, and
    /// a span it enabled would format its fields at each entry, an allocation in every frame of
    /// an app that runs in one, as the stepper runs each app's frame.
    fn warnings() -> FilterFn {
        filter::filter_fn(|metadata| metadata.is_event() && *metadata.level() <= Level::WARN)
    }

    impl Drop for Kept {
        fn drop(&mut self) {
            let lines = String::from_utf8(mem::take(&mut *self.lines.lock().unwrap())).unwrap();
            assert!(
                lines.is_empty() || thread::panicking(),
                "the test logged events at Warn or Error that it did not take:\n{lines}"
            );
        }
    }

    /// The JSON lines, as `Logging` writes them to its file, of every event `f` logs on this
    /// thread.
    pub fn capture(f: impl FnOnce()) -> Vec<String> {
        capture_named("trace", f)
    }

    /// The JSON lines, as `Logging` writes them to its file, of every event `f` logs on this
    /// thread that `targets` enables, by tracing's targets syntax, as a file filter of those
    /// directives keeps them.
    pub fn capture_named(targets: &str, f: impl FnOnce()) -> Vec<String> {
        let targets: Targets = targets
            .parse()
            .unwrap_or_else(|error| panic!("{targets:?} names no targets: {error}"));
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let writer = {
            let buffer = Arc::clone(&buffer);
            move || Captured(Arc::clone(&buffer))
        };
        let layer = json_lines(writer).with_filter(targets);
        subscriber::with_default(tracing_subscriber::registry().with(layer), f);
        let bytes = buffer.lock().unwrap().clone();
        String::from_utf8(bytes)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// A writer into a shared buffer.
    #[derive(Debug)]
    struct Captured(Arc<Mutex<Vec<u8>>>);

    impl Write for Captured {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::panic;

    use serde::Deserialize;
    use tracing::{error, info, warn};

    use campfire_threads::Parallel;

    use crate::json_layer::internals::LogCheck;
    use crate::log_event::LogEvent;

    #[derive(Debug, PartialEq, Eq, Deserialize)]
    struct Refused {
        port: u16,
    }

    impl LogEvent for Refused {
        const MESSAGE: &'static str = "refused a link";

        fn log(&self) {
            warn!(port = self.port, "{}", Self::MESSAGE);
        }
    }

    #[test]
    fn a_scoped_check_keeps_its_threads_events_alone() {
        // The body's warning is its check's, and an error another thread logs meanwhile is not;
        // an event it takes leaves the rest, which `untaken` gives once.
        let scoped = LogCheck::scoped(|| {
            Refused { port: 7 }.log();
            Parallel::map(1, |_| error!("another thread's"));
            info!("below the check");
            warn!("kept");
            3
        });
        assert_eq!(scoped.value, 3);
        assert_eq!(scoped.check.take::<Refused>(), [Refused { port: 7 }]);
        let untaken = scoped.check.untaken();
        assert_eq!(untaken.len(), 1, "{untaken:?}");
        assert!(untaken[0].contains("\"kept\""), "{}", untaken[0]);
        assert_eq!(scoped.check.untaken(), Vec::<String>::new());
    }

    #[test]
    fn a_check_fails_on_each_warning_its_test_does_not_take() {
        let check = LogCheck::start();
        info!("opened");
        Refused { port: 4433 }.log();
        assert_eq!(check.take::<Refused>(), [Refused { port: 4433 }]);
        assert_eq!(check.take::<Refused>(), []);

        // A second check shares the first's events, and the first keeps them once it drops.
        let second = LogCheck::start();
        Refused { port: 1 }.log();
        drop(second);
        Refused { port: 2 }.log();
        assert_eq!(
            check.take::<Refused>(),
            [Refused { port: 1 }, Refused { port: 2 }]
        );
        drop(check);

        // The last check's drop fails with each event left at Warn and Error, and not the Info one.
        let failure = panic::catch_unwind(|| {
            let _check = LogCheck::start();
            info!("opened");
            warn!(port = 1, "closed");
            error!("lost");
        })
        .unwrap_err();
        assert_eq!(
            failure.downcast_ref::<String>().unwrap(),
            concat!(
                "the test logged events at Warn or Error that it did not take:\n",
                r#"{"level":"WARN","fields":{"message":"closed","port":1},"#,
                r#""target":"campfire_log::json_layer::tests"}"#,
                "\n",
                r#"{"level":"ERROR","fields":{"message":"lost"},"#,
                r#""target":"campfire_log::json_layer::tests"}"#,
                "\n",
            )
        );

        // A warning with no check alive fails the next check's start, which keeps it.
        warn!("unseen");
        let failure = panic::catch_unwind(LogCheck::start).unwrap_err();
        assert_eq!(
            failure.downcast_ref::<String>().unwrap(),
            concat!(
                "the test logged events at Warn or Error with no check alive:\n",
                r#"{"level":"WARN","fields":{"message":"unseen"},"#,
                r#""target":"campfire_log::json_layer::tests"}"#,
                "\n",
            )
        );
    }
}
