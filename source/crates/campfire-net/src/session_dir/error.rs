use std::error::Error;
use std::fmt;
use std::io;

use campfire_protocol::{DurableError, JournalError, JournalReplayError, SessionPrivateError};
use campfire_runner::StartError;

/// Why a server's data directory does not say which session to restore.
#[derive(Debug)]
pub enum FindError {
    Read(io::Error),
    /// More than one session has no published log: a server runs one session at a time.
    Several,
}

impl fmt::Display for FindError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FindError::Read(error) => write!(f, "could not read the sessions: {error}"),
            FindError::Several => f.write_str("more than one session has no published log"),
        }
    }
}

impl Error for FindError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            FindError::Read(error) => Some(error),
            FindError::Several => None,
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
    Journal(JournalError),
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
            RestoreError::Journal(error) => write!(f, "{error}"),
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
    Publish(DurableError),
}

impl fmt::Display for AbortError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AbortError::Start(error) => write!(f, "the session's match does not start: {error}"),
            AbortError::Publish(error) => write!(f, "could not publish the log: {error}"),
        }
    }
}

impl Error for AbortError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            AbortError::Start(error) => Some(error),
            AbortError::Publish(error) => Some(error),
        }
    }
}
