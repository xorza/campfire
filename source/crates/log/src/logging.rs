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
            error!(path = %path.display(), %error, "CAMPFIRE_LOG names a file that cannot be created");
        }
        for (variable, refused) in refusals {
            if let Some(error) = refused {
                warn!(variable, %error, "the variable holds no filter, so the default filters");
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
            Err(error) => Some(error.to_string()),
            Ok(text) => match EnvFilter::try_new(&text) {
                Ok(filter) => {
                    return ChosenFilter {
                        filter,
                        refused: None,
                    };
                }
                Err(error) => Some(error.to_string()),
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
    use std::io::{self, Write};
    use std::sync::{Arc, Mutex};

    use tracing::subscriber;
    use tracing_subscriber::layer::SubscriberExt;

    use super::json;

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

    use super::*;

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
