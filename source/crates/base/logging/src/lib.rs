//! A binary's log output: text on standard error, and JSON lines into a file on request, which
//! a worker of `store` writes. A binary starts `Logging` once, first thing in `main`, and reads
//! its command line through `Logging::command_line`, which prints clap's help and logs its
//! refusal.

mod error;
mod log_file;
mod logging;

pub use crate::log_file::LogFile;
pub use crate::logging::Logging;
