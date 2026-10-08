use std::env;
use std::ffi::OsString;
use std::io::{self, IsTerminal};
use std::path::Path;

use campfire_common::ExitStatus;
use campfire_store::StreamWriter;
use clap::Parser;
use tracing::{error, warn};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, fmt};

use crate::error::FilterRefused;
use crate::error_report::ErrorReport;
use crate::json_layer::json;
use crate::log_file::{LogFile, LogLines};

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

/// The bytes of lines the log file's worker holds before a thread that logs waits: a megabyte,
/// many frames of a busy server's events, so a thread waits only when the disk falls far behind.
const LOG_BUFFER: usize = 1 << 20;

/// A filter from an environment variable: its own, or the binary's default when it is not set or
/// holds none, with why it holds none.
#[derive(Debug)]
struct ChosenFilter {
    filter: EnvFilter,
    refused: Option<FilterRefused>,
}

impl Logging {
    /// Installs the process's subscriber, and gives the log file, which the binary holds to the
    /// end of `main`. A log file that cannot be created is logged, and the binary runs on
    /// without it.
    pub fn start(self) -> LogFile {
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
        let opened = path
            .as_ref()
            .map(|path| StreamWriter::create("log", Path::new(path), LOG_BUFFER))
            .transpose();
        let (writer, failed) = match opened {
            Ok(writer) => (writer, None),
            Err(error) => (None, Some(error)),
        };
        let file = writer
            .as_ref()
            .map(|writer| json(LogLines(writer.sender())).with_filter(file_filter.filter));
        tracing_subscriber::registry()
            .with(terminal)
            .with(file)
            .init();
        if let (Some(path), Some(error)) = (path, failed) {
            error!(path = %path.display(), error = %ErrorReport::of(&error), "CAMPFIRE_LOG names a file that cannot be created");
        }
        for (variable, refused) in refusals {
            if let Some(refused) = refused {
                warn!(variable, error = %ErrorReport::of(&refused), "the variable holds no filter, so the default filters");
            }
        }
        writer.map_or_else(LogFile::default, LogFile::of)
    }

    /// The command line `args`, the program first, as `P` reads it; else the status the binary
    /// exits with: `Success` once clap printed the help or the version asked for to standard
    /// output, `Failure` when it does not print, and `Usage` for a command line clap refuses,
    /// which is logged. A binary reads its command line here once `start` installed the log.
    pub fn command_line<P: Parser>(
        args: impl IntoIterator<Item = OsString>,
    ) -> Result<P, ExitStatus> {
        let output = match P::try_parse_from(args) {
            Ok(line) => return Ok(line),
            Err(error) if error.use_stderr() => {
                error!(error = %error, "the command line is refused");
                return Err(ExitStatus::Usage);
            }
            Err(output) => output,
        };
        match output.print() {
            Ok(()) => Err(ExitStatus::Success),
            Err(error) => {
                error!(error = %ErrorReport::of(&error), "the help or the version does not print");
                Err(ExitStatus::Failure)
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
            Err(error) => Some(FilterRefused::NotUnicode(error)),
            Ok(text) => match EnvFilter::try_new(&text) {
                Ok(filter) => {
                    return ChosenFilter {
                        filter,
                        refused: None,
                    };
                }
                Err(error) => Some(FilterRefused::NotFilter(error)),
            },
        };
        ChosenFilter {
            filter: EnvFilter::new(default),
            refused,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::env::VarError;

    use serde_json::Value;

    use super::*;
    use crate::json_layer::internals::capture;
    use crate::log_line::{LogLevel, LogLine};

    #[test]
    fn a_variable_that_holds_no_filter_falls_back_and_says_why() {
        let chosen = |value| {
            let chosen = ChosenFilter::of(value, "warn");
            let refused = chosen.refused.map(|refused| match refused {
                FilterRefused::NotUnicode(_) => "not unicode",
                FilterRefused::NotFilter(_) => "not a filter",
            });
            (chosen.filter.to_string(), refused)
        };
        assert_eq!(chosen(Err(VarError::NotPresent)), ("warn".to_owned(), None));
        assert_eq!(
            chosen(Ok("campfire=debug".to_owned())),
            ("campfire=debug".to_owned(), None)
        );
        assert_eq!(
            chosen(Ok("campfire=loud".to_owned())),
            ("warn".to_owned(), Some("not a filter"))
        );
        let unicode = Err(VarError::NotUnicode(OsString::from("x")));
        assert_eq!(chosen(unicode), ("warn".to_owned(), Some("not unicode")));
        // Its report names the case, then the variable's or the filter's own error.
        let refused = ChosenFilter::of(Ok("campfire=loud".to_owned()), "warn").refused;
        let report = ErrorReport::of(&refused.unwrap()).to_string();
        assert!(report.starts_with("the value is no filter: "), "{report}");
    }

    #[test]
    fn a_command_line_reads_or_ends_the_binary_with_its_status() {
        #[derive(Debug, Parser)]
        #[command(name = "tool", version = "1.0")]
        struct Line {
            count: u8,
        }
        let line = |args: &[&str]| ["tool"].iter().chain(args).map(OsString::from).collect();
        let read = |args: &[&str]| {
            let mut read = None;
            let lines = capture(|| read = Some(Logging::command_line::<Line>(line(args))));
            (read.unwrap().map(|line| line.count), lines)
        };
        assert_eq!(read(&["3"]), (Ok(3), vec![]));
        // The version prints `tool 1.0` to standard output, and is no refusal.
        assert_eq!(read(&["--version"]), (Err(ExitStatus::Success), vec![]));

        let (refused, lines) = read(&["300"]);
        assert_eq!(refused, Err(ExitStatus::Usage));
        let [logged] = &lines[..] else {
            panic!("{lines:?}");
        };
        let logged = LogLine::parse(logged).unwrap();
        let clap_error = Line::try_parse_from::<Vec<OsString>, _>(line(&["300"])).unwrap_err();
        assert_eq!(logged.level, LogLevel::Error);
        assert_eq!(
            (&logged.fields["message"], &logged.fields["error"]),
            (
                &Value::from("the command line is refused"),
                &Value::from(clap_error.to_string())
            )
        );
    }
}
