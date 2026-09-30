//! Replays a session log segment and checks its result.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod replay;
mod verified;

pub use replay::Replay;
pub use verified::Verified;
