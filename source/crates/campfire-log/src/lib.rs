//! The binaries' log output: text on standard error, and JSON lines into a file on request. The
//! libraries emit `tracing` events; a binary starts `Logging` once, first thing in `main`. The
//! events a tool reads back from the JSON lines are `LogEvent`s, and `LogLine` reads them. A
//! binary reads its command line through `Logging::command_line`, which prints clap's help and
//! logs its refusal.

#[cfg(feature = "start")]
mod error;
mod error_report;
#[cfg(any(test, feature = "internals", feature = "start"))]
mod json_layer;
mod json_text;
mod log_event;
#[cfg(feature = "start")]
mod log_file;
mod log_line;
#[cfg(feature = "start")]
mod logging;

pub use crate::error_report::ErrorReport;
pub use crate::json_text::JsonText;
pub use crate::log_event::LogEvent;
#[cfg(feature = "start")]
pub use crate::log_file::LogFile;
pub use crate::log_line::{LogLevel, LogLine};
#[cfg(feature = "start")]
pub use crate::logging::Logging;

#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::json_layer::internals::LogCheck;
    pub use crate::log_event::internals::round_trip;
}
