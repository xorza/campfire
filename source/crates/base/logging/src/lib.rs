//! A binary's log output: text on standard error, JSON lines into a file on request, which a
//! worker of `store` writes, and to a console once it starts. A binary starts `Logging` once,
//! first thing in `main`, and reads its command line through `Logging::command_line`, which
//! prints clap's help and logs its refusal.

mod error;
mod log_file;
mod log_tap;
mod logging;

pub use crate::log_file::LogFile;
pub use crate::log_tap::LogTap;
pub use crate::logging::{LogOutputs, Logging};
