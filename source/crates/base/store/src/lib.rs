//! How the engine writes files: durable and secret files, data directories and their locks, and
//! the workers of `threads` that write them, each with its failure, on the OS's promises that
//! `platform` keeps. The bytes of each file are its caller's:
//! this crate depends on no engine crate, and decides no policy for a failure.

#![expect(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    reason = "the one crate that touches files, which clippy.toml denies the others"
)]

mod append_writer;
mod data_dir;
mod dir_entries;
mod durable_dir;
mod durable_file;
mod exchange;
mod input_file;
mod latest_writer;
mod output_file;
mod path_error;
#[cfg(feature = "internals")]
mod pinned_file;
mod run_dir;
mod secret_file;
mod stream_writer;
#[cfg(feature = "internals")]
mod test_dir;

pub use crate::append_writer::AppendWriter;
pub use crate::append_writer::append_watch::AppendWatch;
pub use crate::append_writer::error::{AppendError, AppendOpenError};
pub use crate::append_writer::slow_sync::SlowSync;
pub use crate::data_dir::DataDir;
pub use crate::data_dir::error::DataDirError;
pub use crate::dir_entries::{DirEntries, DirEntry, EntryKind};
pub use crate::durable_dir::DurableDir;
pub use crate::durable_dir::error::{DurableDirCreateError, DurableDirError};
pub use crate::durable_file::DurableFile;
pub use crate::durable_file::error::{DurableCreateError, DurableError};
pub use crate::exchange::Exchange;
pub use crate::input_file::error::ReadError;
pub use crate::input_file::{FileStamp, InputFile, InputRanges, InputStream};
pub use crate::latest_writer::LatestWriter;
pub use crate::output_file::OutputFile;
pub use crate::path_error::PathError;
pub use crate::run_dir::RunDir;
pub use crate::run_dir::error::RunDirError;
pub use crate::secret_file::SecretFile;
pub use crate::stream_writer::StreamWriter;
pub use crate::stream_writer::error::StreamError;
pub use crate::stream_writer::stream_sender::StreamSender;

/// What the tests of the engine's crates use of files: a test's own directory and the files it
/// pins.
#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::pinned_file::PinnedFile;
    pub use crate::test_dir::TestDir;
}
