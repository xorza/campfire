use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::io;

use campfire_protocol::{JournalReplayError, NotJournal, SessionPrivateError};
use campfire_runner::StartError;
use campfire_store::{AppendOpenError, DurableError};

/// Why a server's data directory does not say which session to restore.
#[derive(Debug)]
pub enum FindError {
    Read(io::Error),
    /// More than one session has no published log: a server runs one session at a time.
    Several,
    /// An entry of the sessions' directory is named by no session id.
    Stray(OsString),
}

impl fmt::Display for FindError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FindError::Read(error) => write!(f, "could not read the sessions: {error}"),
            FindError::Several => f.write_str("more than one session has no published log"),
            FindError::Stray(name) => {
                write!(
                    f,
                    "{} names no session in the sessions' directory",
                    name.display()
                )
            }
        }
    }
}

impl Error for FindError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            FindError::Read(error) => Some(error),
            FindError::Several | FindError::Stray(_) => None,
        }
    }
}

/// Why a session's directory does not restore its session.
#[derive(Debug)]
pub enum RestoreError {
    Read(io::Error),
    Private(SessionPrivateError),
    /// The session runs on another engine release than this one, which restores it.
    OtherRelease(String),
    NotJournal(NotJournal),
    Journal(AppendOpenError),
    Replay(JournalReplayError),
}

impl fmt::Display for RestoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RestoreError::Read(error) => write!(f, "could not read the session: {error}"),
            RestoreError::Private(error) => write!(f, "the private record: {error}"),
            RestoreError::OtherRelease(release) => {
                write!(
                    f,
                    "the session runs on release {release}, which restores it"
                )
            }
            RestoreError::NotJournal(error) => write!(f, "the journal file: {error}"),
            RestoreError::Journal(error) => write!(f, "the journal: {error}"),
            RestoreError::Replay(error) => write!(f, "{error}"),
        }
    }
}

impl Error for RestoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            RestoreError::Read(error) => Some(error),
            RestoreError::Private(error) => Some(error),
            RestoreError::OtherRelease(_) => None,
            RestoreError::NotJournal(error) => Some(error),
            RestoreError::Journal(error) => Some(error),
            RestoreError::Replay(error) => Some(error),
        }
    }
}

/// Why an aborted session's log was not published.
#[derive(Debug)]
pub enum AbortError {
    /// The log does not start a match of the server's mode.
    Start(StartError),
    /// A checkpoint taken again did not write its snapshot.
    Snapshot(DurableError),
    Publish(DurableError),
}

impl fmt::Display for AbortError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AbortError::Start(error) => write!(f, "the session's match does not start: {error}"),
            AbortError::Snapshot(error) => write!(f, "could not write a snapshot: {error}"),
            AbortError::Publish(error) => write!(f, "could not publish the log: {error}"),
        }
    }
}

impl Error for AbortError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            AbortError::Start(error) => Some(error),
            AbortError::Snapshot(error) | AbortError::Publish(error) => Some(error),
        }
    }
}

/// Why the session a stop left under a data directory does not read back.
#[derive(Debug)]
pub enum WaitingError {
    Find(FindError),
    Restore(RestoreError),
    /// The directory of a session whose match never started did not go.
    Remove(io::Error),
}

impl fmt::Display for WaitingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WaitingError::Find(error) => write!(f, "{error}"),
            WaitingError::Restore(error) => write!(f, "the session does not restore: {error}"),
            WaitingError::Remove(error) => {
                write!(f, "could not remove a session that never started: {error}")
            }
        }
    }
}

impl Error for WaitingError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            WaitingError::Find(error) => Some(error),
            WaitingError::Restore(error) => Some(error),
            WaitingError::Remove(error) => Some(error),
        }
    }
}
