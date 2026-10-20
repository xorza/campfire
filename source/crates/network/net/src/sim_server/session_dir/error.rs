use std::ffi::OsString;

use campfire_codec::TaggedError;
use campfire_protocol::PublishError;
use campfire_protocol::{JournalReplayError, JournalStreamError, NotJournal, SessionId};
use campfire_store::{AppendOpenError, DurableDirError, DurableError, PathError, ReadError};
use thiserror::Error;

use crate::sim_server::error::MatchBeginError;

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
    #[error("the journal file")]
    NotJournal(#[source] NotJournal),
    #[error("the journal")]
    Journal(#[source] PathError<AppendOpenError>),
    #[error(transparent)]
    Replay(JournalReplayError),
    /// The journal's header names another session than the directory's name.
    #[error("the journal is session {0}'s")]
    OtherSession(SessionId),
    /// The private record's seed chain is not the one the journal's terms commit to.
    #[error("the private record's seed chain is not the one the terms commit to")]
    OtherSeedChain,
}

/// Why an aborted session's log was not published.
#[derive(Debug, Error)]
pub enum AbortError {
    /// The session's match does not replay to where it stopped.
    #[error("the session's match does not replay")]
    Replay(#[source] MatchBeginError),
    #[error("could not publish the log")]
    Publish(#[source] SessionPublishError),
}

/// Why a session's log was not published, by its step, each of which its path's error names.
#[derive(Debug, Error)]
pub enum SessionPublishError {
    /// The journal did not open to read.
    #[error(transparent)]
    Read(PathError<ReadError>),
    /// The journal did not read.
    #[error(transparent)]
    Journal(JournalStreamError),
    /// The journal holds no whole log.
    #[error(transparent)]
    Log(#[from] PublishError),
    /// The logs' directory was not made.
    #[error(transparent)]
    Dir(#[from] PathError<DurableDirError>),
    #[error(transparent)]
    File(#[from] PathError<DurableError>),
}

/// Why a session's directory was not made, by its step, each of which its path's error names.
#[derive(Debug, Error)]
pub enum SessionCreateError {
    /// The sessions' directory or the session's was not made.
    #[error(transparent)]
    Dir(#[from] PathError<DurableDirError>),
    /// The session's private record was not written.
    #[error(transparent)]
    Private(#[from] PathError<DurableError>),
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
    Remove(#[source] PathError<DurableDirError>),
}
