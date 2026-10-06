use std::ffi::OsString;
use std::io;

use campfire_protocol::{JournalReplayError, NotJournal, SessionPrivateError};
use campfire_runner::StartError;
use campfire_store::{AppendOpenError, DurableError};
use thiserror::Error;

/// Why a server's data directory does not say which session to restore.
#[derive(Debug, Error)]
pub enum FindError {
    #[error("could not read the sessions: {0}")]
    Read(#[source] io::Error),
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
    #[error("could not read the session: {0}")]
    Read(#[source] io::Error),
    #[error("the private record: {0}")]
    Private(#[source] SessionPrivateError),
    /// The session runs on another engine release than this one, which restores it.
    #[error("the session runs on release {0}, which restores it")]
    OtherRelease(String),
    #[error("the journal file: {0}")]
    NotJournal(#[source] NotJournal),
    #[error("the journal: {0}")]
    Journal(#[source] AppendOpenError),
    #[error("{0}")]
    Replay(#[source] JournalReplayError),
}

/// Why an aborted session's log was not published.
#[derive(Debug, Error)]
pub enum AbortError {
    /// The log does not start a match of the server's mode.
    #[error("the session's match does not start: {0}")]
    Start(#[source] StartError),
    /// A checkpoint taken again did not write its snapshot.
    #[error("could not write a snapshot: {0}")]
    Snapshot(#[source] DurableError),
    #[error("could not publish the log: {0}")]
    Publish(#[source] DurableError),
}

/// Why the session a stop left under a data directory does not read back.
#[derive(Debug, Error)]
pub enum WaitingError {
    #[error("{0}")]
    Find(#[source] FindError),
    #[error("the session does not restore: {0}")]
    Restore(#[source] RestoreError),
    /// The directory of a session whose match never started did not go.
    #[error("could not remove a session that never started: {0}")]
    Remove(#[source] DurableError),
}
