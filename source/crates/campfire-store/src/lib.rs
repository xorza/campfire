//! How the engine writes files and runs threads: durable and secret files, data directories and
//! their locks, and the worker threads that write them, each with its failure. The bytes of each
//! file are its owner's: this crate depends on no engine crate, and decides no policy for a
//! failure.

#![expect(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    reason = "the one crate that touches files and starts threads, which clippy.toml denies the others"
)]

mod append_writer;
mod data_dir;
mod dir_entries;
mod durable_file;
mod exchange;
mod input_file;
mod latest_writer;
mod output_file;
mod path_error;
mod platform;
#[cfg(any(test, feature = "internals"))]
mod race;
#[cfg(any(test, feature = "internals"))]
mod scratch;
mod secret_file;
#[cfg(any(test, feature = "internals"))]
mod source_files;
mod stream_writer;
mod worker;

pub use crate::append_writer::AppendWriter;
pub use crate::append_writer::append_file::AppendFile;
pub use crate::append_writer::append_watch::AppendWatch;
pub use crate::append_writer::error::{AppendError, AppendOpenError};
pub use crate::append_writer::slow_sync::SlowSync;
pub use crate::data_dir::DataDir;
pub use crate::data_dir::error::DataDirError;
pub use crate::dir_entries::{DirEntries, DirEntry, EntryKind};
pub use crate::durable_file::DurableFile;
pub use crate::durable_file::error::{DurableCreateError, DurableError};
pub use crate::exchange::Exchange;
pub use crate::input_file::error::ReadError;
pub use crate::input_file::{FileStamp, InputFile, InputRanges, InputStream, Stamped};
pub use crate::latest_writer::LatestWriter;
pub use crate::output_file::OutputFile;
pub use crate::path_error::PathError;
#[cfg(any(test, feature = "internals"))]
pub use crate::platform::file_link::FileLink;
pub use crate::platform::owner_only::Exposure;
#[cfg(any(test, feature = "internals"))]
pub use crate::race::Race;
#[cfg(any(test, feature = "internals"))]
pub use crate::scratch::Scratch;
pub use crate::secret_file::SecretFile;
#[cfg(any(test, feature = "internals"))]
pub use crate::source_files::{SourceFile, SourceFiles};
pub use crate::stream_writer::StreamWriter;
pub use crate::stream_writer::stream_sender::StreamSender;
pub use crate::worker::Worker;
