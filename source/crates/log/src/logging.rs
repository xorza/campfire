use std::env;
use std::fs::File;
use std::io::{self, IsTerminal};
use std::sync::Mutex;

use tracing::error;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, fmt};

/// Where a binary logs: to standard error, as text colored only on a terminal, by `RUST_LOG`;
/// and, when `CAMPFIRE_LOG` names a file, also there as JSON lines with their spans, by
/// `CAMPFIRE_LOG_FILTER`. Each filter falls back to the binary's default when its variable is
/// not set.
#[derive(Debug, Clone, Copy)]
pub struct Logging {
    /// The terminal's filter when `RUST_LOG` does not say.
    pub terminal: &'static str,
    /// The file's filter when `CAMPFIRE_LOG_FILTER` does not say.
    pub file: &'static str,
}

impl Logging {
    /// Installs the process's subscriber. A log file that cannot be created is logged, and the
    /// binary runs on without it.
    pub fn start(self) {
        let terminal = fmt::layer()
            .with_writer(io::stderr)
            .with_ansi(io::stderr().is_terminal())
            .with_filter(
                EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(self.terminal)),
            );
        let path = env::var_os("CAMPFIRE_LOG");
        let (file, failed) = match path.as_ref().map(File::create).transpose() {
            Ok(file) => (file, None),
            Err(error) => (None, Some(error)),
        };
        let file = file.map(|file| {
            fmt::layer()
                .json()
                .with_current_span(true)
                .with_span_list(true)
                .with_writer(Mutex::new(file))
                .with_filter(
                    EnvFilter::try_from_env("CAMPFIRE_LOG_FILTER")
                        .unwrap_or_else(|_| EnvFilter::new(self.file)),
                )
        });
        tracing_subscriber::registry()
            .with(terminal)
            .with(file)
            .init();
        if let (Some(path), Some(error)) = (path, failed) {
            error!(path = %path.display(), %error, "CAMPFIRE_LOG names a file that cannot be created");
        }
    }
}
