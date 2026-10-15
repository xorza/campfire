use campfire_common::{StateHash, Tick};
use thiserror::Error;

use crate::session::error::ResultMismatch;

/// Why a published log does not replay to what it records. The log is untrusted, so each is an
/// expected failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ReplayError {
    /// The state at the boundary before `tick`, where segment `segment` starts, is not the one its
    /// checkpoint records.
    #[error(
        "segment {segment} starts before tick {tick} at state {logged}, and the replay reaches \
         {replayed}"
    )]
    Checkpoint {
        segment: u32,
        tick: Tick,
        logged: StateHash,
        replayed: StateHash,
    },
    /// The log's result does not hold for the state its replay ends in.
    #[error(transparent)]
    Result(ResultMismatch),
}
