//! Replays a session log, segment by segment, and checks its checkpoints, its snapshots and its
//! result.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod error;
mod replay;
mod verified;

pub use error::{ReplayError, SnapshotCheckError};
pub use replay::Replay;
pub use verified::Verified;
