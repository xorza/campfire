//! How the engine writes files and runs threads: durable and secret files, data directories and
//! their locks, and the worker threads that write them, each with its failure. The bytes of each
//! file are its owner's: this crate depends on no engine crate, and decides no policy for a
//! failure.

#![expect(
    clippy::disallowed_methods,
    reason = "the one crate that writes files and starts threads, which clippy.toml denies the others"
)]

mod append_writer;
mod data_dir;
mod durable_file;
mod exchange;
mod latest_writer;
mod secret_file;
mod worker;

pub use append_writer::AppendWriter;
pub use append_writer::append_file::AppendFile;
pub use append_writer::append_watch::AppendWatch;
pub use append_writer::error::{AppendError, AppendOpenError};
pub use append_writer::slow_sync::SlowSync;
pub use data_dir::DataDir;
pub use data_dir::error::DataDirError;
pub use durable_file::DurableFile;
pub use durable_file::error::DurableError;
pub use exchange::Exchange;
pub use latest_writer::LatestWriter;
pub use secret_file::SecretFile;
pub use secret_file::error::SecretReadError;
pub use worker::Worker;
