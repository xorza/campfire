//! The binaries' log output: text on standard error, and JSON lines into a file on request. The
//! libraries only emit `tracing` events; a binary starts `Logging` once, first thing in `main`.

mod logging;

pub use logging::Logging;
