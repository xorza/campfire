//! The binaries' log output: text on standard error, and JSON lines into a file on request. The
//! libraries emit `tracing` events; a binary starts `Logging` once, first thing in `main`. The
//! events a tool reads back from the JSON lines are `LogEvent`s, and `LogLine` reads them.

mod error_report;
mod log_event;
mod log_line;
mod logging;

pub use crate::error_report::ErrorReport;
pub use crate::log_event::LogEvent;
pub use crate::log_line::{LogLevel, LogLine};
pub use crate::logging::Logging;

#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::log_event::internals::round_trip;
    pub use crate::logging::internals::LogCheck;
}
