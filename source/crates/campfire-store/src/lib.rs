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
mod stream_writer;
mod worker;

pub use crate::append_writer::AppendWriter;
pub use crate::append_writer::append_file::AppendFile;
pub use crate::append_writer::append_watch::AppendWatch;
pub use crate::append_writer::error::{AppendError, AppendOpenError};
pub use crate::append_writer::slow_sync::SlowSync;
pub use crate::data_dir::DataDir;
pub use crate::data_dir::error::DataDirError;
pub use crate::durable_file::DurableFile;
pub use crate::durable_file::error::DurableError;
pub use crate::exchange::Exchange;
pub use crate::latest_writer::LatestWriter;
pub use crate::secret_file::SecretFile;
pub use crate::secret_file::error::SecretReadError;
pub use crate::stream_writer::StreamWriter;
pub use crate::stream_writer::stream_sender::StreamSender;
pub use crate::worker::Worker;
