use campfire_package::ContentError;
use campfire_package_disk::StoreError;
use campfire_protocol::LogError;
use campfire_runner::{ReplayError, SnapshotCheckError, StartError, TermsError};
use campfire_store::{PathError, ReadError};
use thiserror::Error;

/// Why a log file does not verify, by the step that failed.
#[derive(Debug, Error)]
pub enum VerifyError {
    /// The directory of packages does not scan.
    #[error("the packages do not scan")]
    Store(#[source] ContentError),
    #[error("the log does not read")]
    ReadLog(#[source] PathError<ReadError>),
    #[error("the log does not decode")]
    Decode(#[source] LogError),
    /// The log's terms are of another engine release, whose packages need not read in this one.
    #[error("the log is of another release")]
    Release(#[source] TermsError),
    /// The store does not give the packages the log's terms name.
    #[error("the log's packages are not held")]
    Packages(#[source] StoreError),
    /// The log's match does not start on the packages it names.
    #[error("the replay does not start")]
    Start(#[source] StartError),
    #[error("a checkpoint's snapshot does not read")]
    ReadSnapshot(#[source] PathError<ReadError>),
    #[error("a checkpoint's snapshot does not check")]
    Snapshot(#[source] SnapshotCheckError),
    /// The replay does not reach what the log records.
    #[error("the log does not replay to what it records")]
    Replay(#[source] ReplayError),
}
