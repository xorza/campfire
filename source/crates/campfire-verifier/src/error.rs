use std::error::Error;
use std::fmt;

use campfire_common::{StateHash, Tick};
use campfire_runner::ResultMismatch;
use campfire_sim::SnapshotError;

/// Why a published log does not replay to what it records. The log is untrusted, so each is an
/// expected failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayError {
    /// The state at the boundary before `tick`, where segment `segment` starts, is not the one its
    /// checkpoint records.
    Checkpoint {
        segment: u32,
        tick: Tick,
        logged: StateHash,
        replayed: StateHash,
    },
    /// The log's result does not hold for the state its replay ends in.
    Result(ResultMismatch),
}

impl fmt::Display for ReplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReplayError::Checkpoint {
                segment,
                tick,
                logged,
                replayed,
            } => write!(
                f,
                "segment {segment} starts before tick {tick} at state {logged}, and the replay \
                 reaches {replayed}"
            ),
            ReplayError::Result(error) => write!(f, "{error}"),
        }
    }
}

impl Error for ReplayError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ReplayError::Checkpoint { .. } => None,
            ReplayError::Result(error) => Some(error),
        }
    }
}

/// Why a snapshot does not hold what a checkpoint record says of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotCheckError {
    /// Its fingerprint is not the record's.
    Fingerprint,
    /// It does not restore.
    Restore(SnapshotError),
    /// It restores to another state than the record's.
    Hash {
        logged: StateHash,
        restored: StateHash,
    },
}

impl fmt::Display for SnapshotCheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SnapshotCheckError::Fingerprint => {
                f.write_str("the snapshot is not the one its checkpoint names")
            }
            SnapshotCheckError::Restore(error) => {
                write!(f, "the snapshot does not restore: {error}")
            }
            SnapshotCheckError::Hash { logged, restored } => write!(
                f,
                "the snapshot restores to state {restored}, and its checkpoint records {logged}"
            ),
        }
    }
}

impl Error for SnapshotCheckError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            SnapshotCheckError::Restore(error) => Some(error),
            SnapshotCheckError::Fingerprint | SnapshotCheckError::Hash { .. } => None,
        }
    }
}
