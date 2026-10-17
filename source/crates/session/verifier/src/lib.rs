//! Replays a session log, segment by segment, and checks its checkpoints, its snapshots and its
//! result.

// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (09-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]

mod events;
mod verification;

pub use crate::events::verified::Verified;
pub use crate::verification::error::VerifyError;
pub use crate::verification::{SnapshotFiles, SnapshotSource, verify_file, verify_log};
