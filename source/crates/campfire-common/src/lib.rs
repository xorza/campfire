//! The vocabulary that crates which do not depend on each other share: the player slot, ticks,
//! segment seeds, secret bytes and 32-byte values of the session log and the sim, and a
//! package's fingerprint.
//!
//! A type enters only when two crates that do not depend on each other both name it, and only
//! as a plain value: construction, parsing, display and serde, no other logic. The crate
//! depends on nothing but `serde` and `derive_more`.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod bytes32;
mod exit_status;
mod fingerprint;
mod player_slot;
mod secret;
mod segment_seed;
mod state_hash;
mod tick;

pub use crate::bytes32::Bytes32;
pub use crate::bytes32::error::NotHex;
pub use crate::exit_status::ExitStatus;
pub use crate::fingerprint::Fingerprint;
pub use crate::player_slot::PlayerSlot;
pub use crate::secret::Secret;
pub use crate::segment_seed::SegmentSeed;
pub use crate::state_hash::StateHash;
pub use crate::tick::{Tick, Ticks};

#[cfg(test)]
mod tests;
