//! Replays a session log segment and checks its result.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod replay;

pub use replay::Replay;
