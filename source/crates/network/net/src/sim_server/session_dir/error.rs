use std::ffi::OsString;

use campfire_codec::TaggedError;
use campfire_common::Version;
use campfire_protocol::{JournalReplayError, NotJournal};
use campfire_store::{AppendOpenError, DurableError, PathError, ReadError};
use thiserror::Error;

use crate::sim_server::error::RestoreMatchError;

/// Why a server's data directory does not say which session to restore.
#[derive(Debug, Error)]
pub enum FindError {
    #[error("could not read the sessions")]
    Read(#[source] PathError<ReadError>),
    /// More than one session has no published log: a server runs one session at a time.
    #[error("more than one session has no published log")]
    Several,
    /// An entry of the sessions' directory is named by no session id.
    #[error("{} names no session in the sessions' directory", .0.display())]
    Stray(OsString),
}

/// Why a session's directory does not restore its session.
#[derive(Debug, Error)]
pub enum RestoreError {
    #[error("could not read the session")]
    Read(#[source] PathError<ReadError>),
    #[error("the private record")]
    Private(#[source] TaggedError),
    /// The session runs on another engine release than this one, which restores it.
    #[error("the session runs on release {0}, which restores it")]
    OtherRelease(Version),
    #[error("the journal file")]
    NotJournal(#[source] NotJournal),
    #[error("the journal")]
    Journal(#[source] PathError<AppendOpenError>),
    #[error(transparent)]
    Replay(JournalReplayError),
}

/// Why an aborted session's log was not published.
#[derive(Debug, Error)]
pub enum AbortError {
    /// The session's match does not replay to where it stopped.
    #[error("the session's match does not replay")]
    Replay(#[source] RestoreMatchError),
    #[error("could not publish the log")]
    Publish(#[source] PathError<DurableError>),
}

/// Why the session a stop left under a data directory does not read back.
#[derive(Debug, Error)]
pub enum WaitingError {
    #[error(transparent)]
    Find(FindError),
    #[error("the session does not restore")]
    Restore(#[source] RestoreError),
    /// The directory of a session whose match never started did not go.
    #[error("could not remove a session that never started")]
    Remove(#[source] PathError<DurableError>),
}
