//! Replays a session log, segment by segment, and checks its checkpoints, its snapshots and its
//! result.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod events;
mod replay;
mod verification;

pub use crate::events::verified::Verified;
pub use crate::replay::Replay;
pub use crate::replay::error::{ReplayError, SnapshotCheckError};
pub use crate::verification::Verification;
pub use crate::verification::error::VerifyError;
