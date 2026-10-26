//! Replays a session log whole and checks its checkpoints, its snapshots and its result; and
//! replays a player's bug report to the tick its client drew, and tables its units there.

// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (07-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]

mod events;
mod report_replay;
mod verification;

pub use crate::events::verified::Verified;
pub use crate::report_replay::error::ReportReplayError;
pub use crate::report_replay::{ReportProbe, replay_report};
pub use crate::verification::error::VerifyError;
pub use crate::verification::{SnapshotSource, verify_file, verify_log};
