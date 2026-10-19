use tracing::Subscriber;
use tracing_subscriber::fmt;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::fmt::format::{Format, Json, JsonFields};
use tracing_subscriber::registry::LookupSpan;

/// The file's layer: an event a line, as JSON, with its spans.
pub(crate) fn json<S, W>(writer: W) -> fmt::Layer<S, JsonFields, Format<Json>, W>
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

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::fmt::Debug;
    use std::io::{self, Write};
    use std::mem;
    use std::sync::{Arc, Mutex, OnceLock, PoisonError, Weak};
    use std::thread;

    use tracing::level_filters::LevelFilter;
    use tracing::subscriber;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::{Layer, Registry};

    use crate::json_layer::json;
    use crate::log_event::LogEvent;
    use crate::log_line::LogLine;
    use campfire_common::ErrorReport;

    /// Asserts that `event` logs one JSON line, which reads back as `event`.
    pub fn round_trip<E: LogEvent + PartialEq + Debug>(event: &E) {
        let lines = capture(|| event.log());
        assert_eq!(lines.len(), 1, "{lines:?}");
        let line = LogLine::parse(&lines[0]).unwrap();
        let read = line.read::<E>().expect("the line is the event");
        assert_eq!(read.as_ref().ok(), Some(event), "{}: {read:?}", lines[0]);
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
        /// The process's check: the one its fixtures share, or a new one, which keeps none of the
        /// events before it; the first makes the process's subscriber, unless a binary's log
        /// holds the capture already.
        pub fn start() -> Self {
            let mut shared = SHARED.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(held) = shared.upgrade() {
                return Self(held);
            }
            if LINES.get().is_none() {
                tracing_subscriber::registry()
                    .with(capture_layer())
                    .try_init()
                    .expect("a process's subscriber is a check's alone");
            }
            let lines = Arc::clone(LINES.get().expect("the capture keeps its lines"));
            lines.lock().unwrap().clear();
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
            let layer = json(writer).without_time().with_filter(LevelFilter::WARN);
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
    pub(crate) fn capture_layer() -> Box<dyn Layer<Registry> + Send + Sync> {
        let lines = Arc::new(Mutex::new(Vec::new()));
        assert!(
            LINES.set(Arc::clone(&lines)).is_ok(),
            "a process holds one capture"
        );
        let writer = move || Captured(Arc::clone(&lines));
        Box::new(json(writer).without_time().with_filter(LevelFilter::WARN))
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
    pub(crate) fn capture(f: impl FnOnce()) -> Vec<String> {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let writer = {
            let buffer = Arc::clone(&buffer);
            move || Captured(Arc::clone(&buffer))
        };
        subscriber::with_default(tracing_subscriber::registry().with(json(writer)), f);
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

    use campfire_store::internals::Parallel;

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
    }
}
