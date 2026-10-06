use campfire_common::{StateHash, Tick};
use campfire_runner::ResultMismatch;
use campfire_sim::SnapshotError;
use thiserror::Error;

/// Why a published log does not replay to what it records. The log is untrusted, so each is an
/// expected failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ReplayError {
    /// The state at the boundary before `tick`, where segment `segment` starts, is not the one its
    /// checkpoint records.
    #[error(
        "segment {segment} starts before tick {tick} at state {logged}, and the replay \
                 reaches {replayed}"
    )]
    Checkpoint {
        segment: u32,
        tick: Tick,
        logged: StateHash,
        replayed: StateHash,
    },
    /// The log's result does not hold for the state its replay ends in.
    #[error("{0}")]
    Result(#[source] ResultMismatch),
}

/// Why a snapshot does not hold what a checkpoint record says of it.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SnapshotCheckError {
    /// Its fingerprint is not the record's.
    #[error("the snapshot is not the one its checkpoint names")]
    Fingerprint,
    /// It does not restore.
    #[error("the snapshot does not restore: {0}")]
    Restore(#[source] SnapshotError),
    /// It restores to another state than the record's.
    #[error("the snapshot restores to state {restored}, and its checkpoint records {logged}")]
    Hash {
        logged: StateHash,
        restored: StateHash,
    },
}
