use std::env;
use std::fs::File;
use std::io::{self, IsTerminal};
use std::sync::Mutex;

use tracing::{Subscriber, error, warn};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::fmt::format::{Format, Json, JsonFields};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, fmt};

use crate::error_report::ErrorReport;

/// Where a binary logs: to standard error, as text colored only on a terminal, by `RUST_LOG`;
/// and, when `CAMPFIRE_LOG` names a file, also there as JSON lines with their spans, by
/// `CAMPFIRE_LOG_FILTER`. Each filter falls back to the binary's default when its variable is
/// not set, and when it holds no filter, which the binary then logs.
#[derive(Debug, Clone, Copy)]
pub struct Logging {
    /// The terminal's filter when `RUST_LOG` does not say.
    pub terminal: &'static str,
    /// The file's filter when `CAMPFIRE_LOG_FILTER` does not say.
    pub file: &'static str,
}

/// A filter from an environment variable: its own, or the binary's default when it is not set or
/// holds none, with why it holds none.
#[derive(Debug)]
struct ChosenFilter {
    filter: EnvFilter,
    refused: Option<String>,
}

impl Logging {
    /// Installs the process's subscriber. A log file that cannot be created is logged, and the
    /// binary runs on without it.
    pub fn start(self) {
        let terminal_filter = ChosenFilter::of(env::var(EnvFilter::DEFAULT_ENV), self.terminal);
        let file_filter = ChosenFilter::of(env::var("CAMPFIRE_LOG_FILTER"), self.file);
        let refusals = [
            (EnvFilter::DEFAULT_ENV, terminal_filter.refused),
            ("CAMPFIRE_LOG_FILTER", file_filter.refused),
        ];
        let terminal = fmt::layer()
            .with_writer(io::stderr)
            .with_ansi(io::stderr().is_terminal())
            .with_filter(terminal_filter.filter);
        let path = env::var_os("CAMPFIRE_LOG");
        #[expect(
            clippy::disallowed_methods,
            reason = "tracing writes the file of JSON lines as it logs, outside the store's workers"
        )]
        let (file, failed) = match path.as_ref().map(File::create).transpose() {
            Ok(file) => (file, None),
            Err(error) => (None, Some(error)),
        };
        let file = file.map(|file| json(Mutex::new(file)).with_filter(file_filter.filter));
        tracing_subscriber::registry()
            .with(terminal)
            .with(file)
            .init();
        if let (Some(path), Some(error)) = (path, failed) {
            error!(path = %path.display(), error = %ErrorReport::of(&error), "CAMPFIRE_LOG names a file that cannot be created");
        }
        for (variable, refused) in refusals {
            if let Some(refused) = refused {
                warn!(variable, error = %refused, "the variable holds no filter, so the default filters");
            }
        }
    }
}

impl ChosenFilter {
    /// The filter `value`, a variable's, gives: `default` when the variable is not set, or when
    /// it holds no filter, with why.
    fn of(value: Result<String, env::VarError>, default: &str) -> ChosenFilter {
        let refused = match value {
            Err(env::VarError::NotPresent) => None,
            Err(error) => Some(ErrorReport::of(&error).to_string()),
            Ok(text) => match EnvFilter::try_new(&text) {
                Ok(filter) => {
                    return ChosenFilter {
                        filter,
                        refused: None,
                    };
                }
                Err(error) => Some(ErrorReport::of(&error).to_string()),
            },
        };
        ChosenFilter {
            filter: EnvFilter::new(default),
            refused,
        }
    }
}

/// The file's layer: an event a line, as JSON, with its spans.
fn json<S, W>(writer: W) -> fmt::Layer<S, JsonFields, Format<Json>, W>
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

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use std::cell::RefCell;
    use std::io::{self, Write};
    use std::mem;
    use std::rc::{Rc, Weak};
    use std::sync::{Arc, Mutex};
    use std::thread;

    use tracing::dispatcher::DefaultGuard;
    use tracing::level_filters::LevelFilter;
    use tracing::subscriber;
    use tracing_subscriber::Layer;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    use super::json;
    use crate::error_report::ErrorReport;
    use crate::log_event::LogEvent;
    use crate::log_line::LogLine;

    thread_local! {
        /// The check the fixtures of the thread share, while one of them holds it.
        static SHARED: RefCell<Weak<Kept>> = const { RefCell::new(Weak::new()) };
    }

    /// The events at Warn and Error that the code a test runs logs on its thread, records of the
    /// `log` crate among them. A test takes those it causes; when the thread's last check drops,
    /// each event left fails the test, as the LAN check fails a match that logs one.
    #[derive(Debug)]
    pub struct LogCheck(Rc<Kept>);

    /// The thread's subscriber, and the events it kept, as JSON lines.
    #[derive(Debug)]
    struct Kept {
        lines: Arc<Mutex<Vec<u8>>>,
        _subscriber: DefaultGuard,
    }

    impl LogCheck {
        /// The thread's check: the one its fixtures share, or a new one, which becomes the
        /// thread's subscriber.
        pub fn start() -> LogCheck {
            SHARED.with(|shared| {
                let held = shared.borrow().upgrade();
                LogCheck(held.unwrap_or_else(|| {
                    let kept = Rc::new(Kept::install());
                    *shared.borrow_mut() = Rc::downgrade(&kept);
                    kept
                }))
            })
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

    impl Kept {
        /// Makes the thread's subscriber one that keeps its events at Warn and Error, with no
        /// time, so a failure names the same lines in every run.
        fn install() -> Kept {
            let lines = Arc::new(Mutex::new(Vec::new()));
            let writer = {
                let lines = Arc::clone(&lines);
                move || Captured(Arc::clone(&lines))
            };
            let layer = json(writer).without_time().with_filter(LevelFilter::WARN);
            Kept {
                lines,
                _subscriber: tracing_subscriber::registry().with(layer).set_default(),
            }
        }
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
    use std::env::VarError;
    use std::ffi::OsString;
    use std::panic;

    use serde::Deserialize;
    use tracing::info;

    use super::*;
    use crate::log_event::LogEvent;
    use crate::logging::internals::LogCheck;

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
                r#""target":"campfire_log::logging::tests"}"#,
                "\n",
                r#"{"level":"ERROR","fields":{"message":"lost"},"#,
                r#""target":"campfire_log::logging::tests"}"#,
                "\n",
            )
        );
    }

    #[test]
    fn a_variable_that_holds_no_filter_falls_back_and_says_why() {
        let chosen = |value| {
            let chosen = ChosenFilter::of(value, "warn");
            (chosen.filter.to_string(), chosen.refused.is_some())
        };
        assert_eq!(
            chosen(Err(VarError::NotPresent)),
            ("warn".to_owned(), false)
        );
        assert_eq!(
            chosen(Ok("campfire=debug".to_owned())),
            ("campfire=debug".to_owned(), false)
        );
        assert_eq!(
            chosen(Ok("campfire=loud".to_owned())),
            ("warn".to_owned(), true)
        );
        let unicode = Err(VarError::NotUnicode(OsString::from("x")));
        assert_eq!(chosen(unicode), ("warn".to_owned(), true));
    }
}
