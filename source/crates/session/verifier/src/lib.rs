//! Replays a session log, segment by segment, and checks its checkpoints, its snapshots and its
//! result.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod events;
mod verification;

pub use crate::events::verified::Verified;
pub use crate::verification::Verification;
pub use crate::verification::error::VerifyError;
