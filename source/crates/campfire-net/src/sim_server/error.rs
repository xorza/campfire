use std::io;

use campfire_protocol::{ConnectError, DelegationError};
use campfire_runner::{ResumeError, ServerInputRefused, StartError};
use campfire_store::DurableError;
use thiserror::Error;

/// Why the server refused a player's join.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum JoinError {
    #[error("{0}")]
    Delegation(#[source] DelegationError),
    #[error("{0}")]
    Connect(#[source] ConnectError),
    /// Every slot was taken by the time the join arrived.
    #[error("every slot is taken")]
    Full,
    /// After the start, no slot is the player's, open to them or a bot's they may take.
    #[error("no slot is the player's or open to them")]
    NoSlot,
    /// The log refused what the join would change.
    #[error("{0}")]
    Refused(#[source] ServerInputRefused),
    /// A newer login of the player took the link's seat.
    #[error("a newer login of the player took the seat")]
    Superseded,
}

/// Why a session a stop ended does not restore its match.
#[derive(Debug, Error)]
pub enum RestoreMatchError {
    /// The log does not start a match of the server's mode.
    #[error("the match does not start: {0}")]
    Start(#[source] StartError),
    /// The latest checkpoint's snapshot does not read.
    #[error("the latest checkpoint's snapshot does not read: {0}")]
    ReadSnapshot(#[source] io::Error),
    /// The latest checkpoint's snapshot does not resume the match.
    #[error("the match does not resume: {0}")]
    Resume(#[source] ResumeError),
    /// A checkpoint taken again did not write its snapshot.
    #[error("a checkpoint's snapshot was not written: {0}")]
    WriteSnapshot(#[source] DurableError),
}
